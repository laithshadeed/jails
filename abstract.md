# abstract.md — the mutation architecture contract

`plan.md` says what to change and in what order. This document says what the mutation system is, which
guarantees it must provide, and which abstractions it must not pretend to have.

This is the **target contract**, not a claim that the current code already satisfies it. The
audited baseline, queued phases and exact implementation/wire details live in `plan.md`. Any implementation
choice that weakens this contract requires both documents and their enforcement tests to change together.

This is a living contract, not a historical audit. Measurements and delivery status belong in tests and
`plan.md`; superseded designs belong in git history.

### Legacy citation locator

Source and test comments written against the pre-contract document still cite its old sections. Until those
callers naturally change, read the citations as these conceptual aliases:

| Earlier citation | Current home |
|---|---|
| §2 | Current evidence in §2; design test in §§8–9 |
| §3, especially §3.2 | Current module evidence in §2; snapshot boundary in §3.1; decomposition rule in §8 |
| §4.1 | Current `Artifact`/`Change` limit in §2; desired/prepared changes in §§3.3–3.4; removal in §7 |
| §4.2 | Interpreter gap in §2; shared planning in §4; derived verification in §9 |
| §4.3 | Snapshot/project boundary in §3.1; planning purity in §4 |
| §4.4 | Typed identity and references in §3.2 |
| §4.5 | Ownership boundary in §1; canonical ledger in §6 |
| §5 | Clone-the-abstraction diagnosis in §2; governing principle in §8 |
| §6.2 | Current derived behaviour in §2; consequences in §§4, 7, and 9 |
| §6.3 | Human/machine ownership in §1; ledger semantics in §6 |
| §7 | Delivery sequence in `plan.md`; architectural destination in §§3–7; gates in §9 |
| §8 | Wrong-abstraction counterweight in §8; falsification through §9 |
| §8.0 | Ratchet policy in §9 and `tests/architecture.rs` |
| §8.0.1 | Current doctor evidence in §2; counter policy in §9 |
| §8.1 | Current-state evidence in §2; delivery state in `plan.md`; ratchets in §9 |
| §8.2 | Human manifests in §1; ledger boundary in §6 |
| §9 | Principles in §8; enforceable contract in §9 |

## 0. The one sentence

jails turns a typed mutation request and an immutable view of a project into a
**prepared, recoverable transition** of that project—or one guarded retry of an
already-recorded external effect.

```text
load       : (ProjectHandle, HumanSourceSelection, DirectRequest)
             -> LoadedProject
resolve    : (&LoadedProject, DirectRequest) -> ResolvedMutation
plan_all   : (&LoadedProject, ResolvedMutation) -> PlannedTransition
prepare    : (LoadedProject, CommitPlan, PreparationContext) -> PreparedBundle
describe   : PreparedBundle.change -> Report
describe_effect : EffectRetryPlan -> EffectRetryReport       // after = None
describe_effect_result : (EffectRetryPlan, EffectRunResult) -> EffectRetryReport
commit     : (ProjectHandle, PreparedBundle) -> Result<CommitResult, CommitError>
resume_effect : (ProjectHandle, EffectRetryPlan) -> Result<EffectRunResult, EffectRunError>
recover    : ProjectHandle -> Result<RecoveryOutcome, RecoveryError>
verify_project : ProjectSnapshot -> [Finding]
```

`load` owns a staged, ledger-first capture. With no pending conflict it returns
`LoadedProject::Ready`: captured declaration syntax plus the complete ordinary
snapshot. With a committed pending conflict it returns the deliberately
smaller, parse-free `LoadedProject::Pending`. That fast path does not feed a
marker-bearing POM, `jails.toml`, app manifest or Java source through an
ordinary parser. It uses the current request-syntax fingerprint and frozen
desired-input guards to reach only guarded continue or abort. A caller never
constructs or supplies the final `InputSet`, and no planner may expand it by
reading live state. `plan.md` defines the exact bootstrap and completeness
algorithm.

`PlannedTransition` is a closed choice between `Commit(CommitPlan)` and
`RetryEffect(EffectRetryPlan)`. A `CommitPlan` is exactly one of ordinary
apply, conflict finalisation, or guarded conflict abort. Only `commit` and
`recover` mutate managed project files or the ledger. `resume_effect` is the
executor-owned entrypoint that compare-and-swaps receipt effect state and runs
one already-recorded external effect; it never plans or changes project files.
`plan_all` is pure. `prepare` accepts the matching `LoadedProject` variant,
moves its opaque runtime bindings into the prepared bundle, and may perform
fallible work only in memory or bounded scratch space. It does not mutate the
project. `describe` reports the exact operations `commit` will attempt.

`destroy` and `remove` are not a generic inverse function. They plan the desired project with an owner claim
removed. A receipt supports crash recovery, conflict abort and audit; it is not the semantic model of
uninstall or a universal rollback API.
`verify_project` covers captured project state. Machine, process, socket and environment probes remain
separate read-only services; the mutation snapshot must not be stretched into a universal inspection model.

## 1. Scope and ownership

This contract governs capability add/remove, artifact generate/destroy, app apply/reconcile, field, factory,
sync, and related changes to generated code, build files, compose, properties, human configuration, and
machine state.

Read-only commands may use simpler paths. They must still fail closed when authoritative state cannot be
read, and a command presented as read-only must not migrate or repair state as a side effect.

Authority is split by domain; “authoritative” never means “all of these files
describe the same thing”:

1. `jails.toml` and `.jails/app.toml` are human-owned declarations. jails may make explicit surgical edits
   while preserving unrelated bytes, but must not rewrite them as machine state.
2. `.jails/ledger.toml` is authoritative for the current logical ownership,
   provenance and pending-conflict generation. It is not an execution log.
3. One validated active journal is authoritative for how an incomplete
   transaction may advance. A retained receipt is authoritative for its
   immutable prepared history and mutable effect state. Neither is consulted
   to invent current desired state.
4. Content-addressed objects are authoritative only for the bytes named by an
   already-authoritative ledger, journal or receipt reference. Reachability,
   not directory presence, gives an object meaning.
5. Project files are shared. Some are wholly generated; others contain managed contributions beside user
   content. Ownership is recorded per managed resource, not inferred merely from a path.

Legacy formats are compatibility inputs only. There is no permanent dual write. Any compatibility
projection is derived, transitional, and non-authoritative.

## 2. What exists now

The current code has valuable foundations, but none should be described more strongly than it is:

| Foundation | Current truth | Remaining limit |
|---|---|---|
| `model::Project` | Central resolved project facts and layers | Some planning still rereads live disk |
| `model::Artifact` | One file shape with eagerly rendered contents | A complete change includes more than files |
| `model::Change` | Shared add/generate shape and fallible preflight merge | Omits deletes, state, ownership, registration, and effects |
| `generate::artifacts_for` | Common generation query; `KIND_FILES` is gone | Removal still has fallbacks and direct mutation |
| `codemod::Marked` | One owner for marked-block syntax | No general semantic-edit model or interpreter |
| `apply` | Direct write primitives and atomic replacement of one file | Not yet the owner of deletes, copies, mkdir, or mutating tools |
| `.jails/ledger.toml` | One canonical entity registry | Not a journal, provenance store, or exact merge-base store |
| Module splits | Focused recipe, app, doctor, add, and Spring modules | File layout alone does not unify mutation flow |

`Change::merge` is partial and fallible, not a monoid. Add uses its result for preflight and then discards it
in favour of a separate imperative apply path. Generate writes files, ledger state, command registration,
and build changes in separate steps; app apply and reconciliation are likewise multi-step.

The current meaning of planner “purity” is often only “does not write”. Some planners read live files.
Legacy migration can happen on a read path, some migration is lossy, and ledger read errors other than
`NotFound` can be mistaken for empty state. Reconciliation reconstructs an old base with the current
renderer, which is not exact after templates, tool versions, or relevant project context change.

The target below completes these seams. It does not create a second framework beside the existing types.

## 3. The internal model

The names are conceptual. Existing types should evolve into these roles rather than being wrapped forever
by parallel representations.

### 3.1 Snapshot and projected view

`ProjectSnapshot` contains every planning input: resolved project root, build kind/flavour, Java release,
layers and package, parsed build/config/compose/property/source facts, observed ledger state, relevant file
bytes and existence facts, and frozen resolved template bytes and origins. Its canonical `ReadSet` records
every file image or absence, every enumerated directory and its sorted entries, and every allowed external
manifest/template/brief input. It also loads and verifies the complete transitive closure of objects referenced by the ledger;
planners never read the object store lazily. Those entries become commit preconditions; an optional absent file and an empty
directory are therefore observable facts rather than gaps in the snapshot.

Once receipts exist, loading also freezes the bounded retained receipt
inventory and its directory digest. Effect retry plans use those captured
records. A pending conflict requires exactly one structurally matching origin
receipt and loads its abort preimages. Receipt checksum and inventory rows are
commit preconditions; planners never rescan receipt storage lazily.
`MachineReceiptDirectoryState` distinguishes an absent receipts directory from
a present directory with its sorted listing and digest; present-empty is not
encoded as absence.

R4's receipt support is deliberately ordered before ledger-object resolution.
The loader first validates every retained Complete-journal/receipt pair and its
entire local prepared manifest. A ledger object then records an exact
`MachineObjectSource`: prefer the verified global store; during the pre-R5 dark
period only, fall back to the lowest-`TransactionId` fully validated receipt
whose manifest and local object set contain it. That chosen global or receipt-
local location is part of `InputPrecondition::MachineObject`, and commit reopens
and rehashes that exact source. A missing/corrupt selected source is stale or
corrupt input even if another copy later appears; execution never switches
sources after planning. Before receipt validation exists, resolution is global-
only and no production schema-2 writer is active.

`LoadedSnapshot` pairs that deterministic value with opaque runtime absolute bindings for allowed external
inputs. `LoadedProject` keeps ordinary and pending loading impossible to confuse:

```rust
enum LoadedProject {
    Ready {
        loaded: LoadedSnapshot,
        declarations: DeclarationSyntax,
    },
    Pending {
        loaded: LoadedPendingSnapshot,
    },
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
```

Loading captures and strictly decodes the ledger before parsing the build,
human declarations or source facts. If that ledger contains a pending
conflict, the loader validates the origin receipt and Complete journal, captures
only the frozen affected paths and desired inputs, and returns `Pending`.
`RequestSyntaxFingerprint` is derived from current CLI syntax without
project-derived defaults. Only after it, the manifest source identity and every
desired-input guard match may pending resolution reuse the stored canonical
request. `Exact` rehashes independent captured bytes,
`ProjectedTransactionOutput` is accepted only for a path already guarded by the
origin transaction, and every relevant absence has an explicit `Absent` row.
Pending mode may parse a marker-free resolved shared file only to validate its
frozen semantic slots; it never runs the ordinary bootstrap parsers.

Only preparation moves runtime bindings into
`PreparedBundle.commit_context`; planners, reports, fingerprints, journals and
receipts never see or persist them. `Apply` requires `Ready`, while `Finalise`
and `Abort` require `Pending`; every other pairing is an invariant violation.

A snapshot method never reads the filesystem, environment, clock, or a process. `ProjectHandle` owns the
root and is used only by loading and execution.

Resolved templates belong only to the snapshot. `PreparationContext` supplies renderer/tool fingerprints,
bounded tool specifications, and the scratch executor used to materialise bytes. Anything from it that can
affect output contributes to the prepared fingerprint, but commit never reruns those tools.

Planning several intents uses a `ProjectedProject`: the snapshot plus changes already desired earlier in the
plan. Intents are resolved in stable topological order and the projected view advances after each. A later
intent sees a dependency introduced earlier without an intermediate write or fresh disk read.

### 3.2 Typed intent and identity

Persistent intent identity is canonical and independent of mutable arguments:

```rust
struct IntentId {
    recipe: Recipe,
    name: Name,
    package: Package,
}

enum EntityId {
    Capability(CapabilityId),
    Intent(IntentId),
    ToolFeature(ToolFeature),
}

enum ResourceOwner {
    Entity(EntityId),
    OneShot(OneShotId),
}

enum OwnerId { AppManifest, DirectConfig, DirectCli }

enum ReconcileScope {
    AppManifest,
    DirectConfig,
    DirectEntity(EntityId),
}

struct DesiredEntity {
    id: EntityId,
    spec: EntitySpec,
    owners: BTreeSet<OwnerId>,
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

enum PlannedTransition {
    Commit(CommitPlan),
    RetryEffect(EffectRetryPlan),
}

enum CommitPlan {
    Apply(DesiredChangeSet),
    Finalise(FinalisationPlan),
    Abort(AbortPlan),
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
```

Optional package/name spellings exist only in syntax DTOs. Resolution applies conventions and validates
them before constructing these types. Recipe arguments remain recipe-specific. `on` and `yields` are typed
references to compatible managed or captured-existing targets, not one string whose meaning changes by
recipe. Before preparation the planner rejects duplicate identities, missing or incompatible references,
and cycles. One-shot field, migration and case operations have
`OneShotId`/receipts; they are deliberately not persistent desired entities.
Only cases has a destroy route. App initialisation, rename, adoption and
formatting are typed maintenance subjects, not fake entities. After the
pending-conflict gate, exactly one same-invocation eligible interrupted or
failed effect becomes `RetryEffect` and cannot also produce a fresh project
transaction. Pending conflict continue/abort becomes
`Finalise`/`Abort`; ordinary actions cannot reach those prepared kinds.

Manifest aliases may remain at the parser boundary; they do not survive into the typed model. A future
runtime descriptor must serialise these same types rather than create another recipe schema beside them.

`DesiredState` comes only from the captured human `jails.toml`, the selected
captured app manifest, and the current direct request. Ledger rows are observed
state inside `ProjectSnapshot`; they are never translated into declarations.
The other `ResolvedAction` variants may inspect captured observed state to
construct guarded transitions, but they do not call that state desire. App
ownership remains an `AppManifest` ledger claim and is never copied into
`jails.toml`.

`ResourceKey::HumanConfigCapability(id)` exists if and only if that entity has
the `DirectConfig` owner. Its sole resource owner is the entity and its value is
the exact capability spec. `AppManifest` and `DirectCli` owners never emit that
resource or copy their capability into `jails.toml`; removing `DirectConfig`
removes only the declaration resource when another owner keeps the entity
alive. On the first schema-2 transition only, an existing valid `jails.toml`
whose parsed `DirectConfig` declaration/resource is exact and whose pure editor
would make no byte change permits a ledger-only authoritative bootstrap: emit
no `FileOp`, and record the exact live file as base/current with a truthful
`FormatOwner::HumanConfig`. A mismatch, duplicate, required edit, prior V2
config ownership or competing managed record follows ordinary collision and
stored-base rules; this is not general adoption.

That first-V2 authority has one matching resource bootstrap. It applies only
to the **complete** resource/output closure of an entity declared by captured
`DirectConfig`, plus `ToolFeature::FastTest` only for the explicit current
`test --fast` request. Every ID/spec/prerequisite must come from that real
owner, every semantic key must occur exactly once with the exact desired value,
no candidate V2 record may have existed, and the complete fresh format-owner
edit/render must be unchanged byte-for-byte and mode-for-mode. The closure is
all-or-nothing. Success emits no `FileOp`, creates only the real ownership rows,
and records exact live base/current images with fresh truthful renderer
contexts. Partial, unequal, duplicate, post-V2, app-only or legacy-only
candidates refuse; live coincidence never manufactures an owner.

Desired/observed comparison is scoped replacement. Planning first discards the
active scope's old observed claim, then inserts that scope's current claim when
present and retains every outside owner. A sole owner may therefore update its
spec; multiple current/retained claims may update together only when they agree
on one canonical spec. An incompatible outside claim refuses with an owner and
field-level diff. A retained outside owner takes a new spec only from a
participating captured authoritative source that explicitly declares that same
identity; otherwise its observed spec remains. Omission outside the active
scope is never removal.

Capability prerequisites are validation edges, not declarations. Every
transitive prerequisite must have a real current or retained `OwnerId`; files,
dependencies and live services do not satisfy the edge, and the planner never
invents a synthetic owner. Last-owner removal refuses while any retained
entity depends on that capability.

### 3.3 Desired change

The current `Change` should become an exhaustive desired change:

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
```

`SemanticEdit` covers decisions that compose by meaning before becoming bytes: POM dependencies and
plugins, compose services, property keys, marked source blocks, command registration, and surgical edits to
human configuration.

`ChangeAttribution::Resource` is limited to an entity or one-shot and may add
durable resource ownership. `Maintenance` is audit attribution, not a fake
owner. A subject-specific maintenance planner may transform resources whose
owners remain real entities/one-shots (for example rename or adoption), or
guard an explicitly unowned human/shared file such as app initialisation or
standalone formatting. The maintenance tag itself can never be a contributor.
Such an unowned file is reported and
receipted, but creates no `OutputRecord` and is never later deleted by owner
reconciliation.

`AdoptLayout` is a human delta, not ownership adoption. If `jails.toml` already
has a managed output, its contributors, generated base and renderer remain
unchanged while only `current` advances to the exact committed postimage. With
no managed config output, the edit remains unowned and creates no output row or
`HumanConfigCapability` resource.

Every managed output has a `ResourceKey` and owner set. A key may name a whole file or a semantic contribution such
as a Maven coordinate, compose service, property key, or marked block. Two entities can therefore share a
dependency without either owning its deletion. `ResourceKey`, not a path or an intent, is the deletion unit.
Whole-file ownership cannot coexist with semantic contribution ownership at the same path.

```text
try_compose : [DesiredChange] -> Result<DesiredChangeSet, Conflict>
```

`DesiredChangeSet` also carries one closed `PlannedSubject` identifying the
ordinary action whose changes it contains. This lets operation identity
distinguish reconciliation, a one-shot, app initialisation and maintenance
without accepting an untyped bag of changes. `FinalisationPlan`, `AbortPlan`
and `EffectRetryPlan` remain separate because their preconditions and allowed
effects are different.

Composition detects incompatible claims and combines compatible shared contributions. Associativity may be
tested where meaningful; totality, identity, and a mathematical monoid are not claimed.

### 3.4 Prepared change and receipt

Preparation lowers semantic edits into exact bytes and guarded operations:

```rust
enum FileImage {
    Absent,
    Present { object: ObjectRef, mode: FileMode },
}

struct GuardedImage {
    object: ObjectRef,
    mode: FileMode,
}

struct PreparedChange {
    format: u32,
    operation_identity: OperationIdentityV1,
    operation_id: OperationId,
    transaction_id: TransactionId,
    preparation: PreparationContextFingerprint,
    input_preconditions: Vec<InputPrecondition>,
    operations: Vec<FileOp>,
    directories: Vec<DirectoryOp>,
    ledger_before: FileImage,
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

struct OperationContextFingerprint {
    schema: u32,
    tools: Vec<OperationToolFingerprint>,
}

struct OperationToolFingerprint {
    identity: ToolIdentityFingerprint,
    args: Vec<ToolArgTemplate>,
}

enum ToolArgTemplate {
    Literal(String),
    OperationLabel { prefix: String, hex_chars: u8 },
}

enum OperationTarget {
    Project(ProjectPath),
    LegacyMachine(LegacySourcePath),
}

enum FileOp {
    Create { path: OperationTarget, after: ObjectRef, mode: FileMode,
             contributors: BTreeSet<ResourceOwner> },
    Replace { path: OperationTarget, before: GuardedImage, after: ObjectRef,
              mode: FileMode, contributors: BTreeSet<ResourceOwner> },
    Delete { path: OperationTarget, before: GuardedImage,
             contributors: BTreeSet<ResourceOwner> },
}

enum DirectoryOp { Create { path: ProjectPath } }

struct PreparedBundle {
    change: PreparedChange,
    commit_context: CommitContext, // runtime-only; never persisted/reported
}

struct CommitContext {
    project_root: RootIdentity,
    external_inputs: BTreeMap<ExternalInputId, ExternalBinding>,
    machine_root: MachineRootBinding,
}

struct FileReceipt {
    path: OperationTarget,
    before: FileImage,
    after: FileImage,
    contributors: BTreeSet<ResourceOwner>,
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
    receipt: AppliedReceipt,
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

`DesiredFile.mode` is the mutation model's only optional mode and expresses
policy before preparation. `Some` is an explicit permission requirement;
`None` preserves the captured concrete live mode for a replace and resolves to
`0o644` for a create. Executable output must request its concrete mode, normally
`0o755`. Every snapshot/read precondition, stored/live/guarded/prepared/actual
file image and receipt therefore carries a concrete `FileMode`. Preparation
resolves the policy once; the executor sets and verifies the exact bits, so
results are independent of process umask.

The first schema-1-to-2 transition is an explicit protocol bridge, not an
ordinary project edit. `OperationSemanticsV1::Apply` carries
`Option<LegacyMigrationIdentity>` whose immutable snapshot names every closed
legacy source as absent or as an exact object/mode, both legacy directory
listings, and the complete purely translated `LedgerV2Draft`. Preparation and
durable validation rerun the same legacy translation over those objects and
require byte-for-value equality with the draft; they do not attempt to parse a
schema-1 `ledger_before` as schema 2.

Only that validated migration may use
`OperationTarget::LegacyMachine`, and only for exact `Delete` operations with
empty contributors covering every present non-ledger legacy source. The
schema-1 ledger path is consumed solely by the guarded ledger transition;
legacy targets are never creates, replacements, renderer outputs, semantic
resources, conflict paths or directory operations. File receipts and reports
use the same `OperationTarget` distinction. Every legacy source and directory
is a read/commit guard. Once schema 2 exists, all old static files must remain
absent and legacy directories must be absent or present-empty; reintroduced
children fail closed.

`RecoveryOutcome` is a sorted in-memory report of structural changes made by
one recovery call and nonterminal effects that were reported but not executed.
`EffectStateChanged` records both allowed structural effect transitions:
obsolete logical guards become `Superseded`, and an orphaned
`Running { attempt >= 2 }` becomes `Failed { code: InterruptedTwice, .. }`.
Every `Running` state is reported; this value is not a durable protocol record.
`RecoveredPriorTransaction(outcome)` tells the outer driver to reload and
replan once; it is not the requested operation's success. The typed error enums
are the only execution-to-command-result boundary: stale input, lock
contention, blocked/corrupt recovery and pre-activation I/O are not collapsed
into an ambiguous string or a partial receipt.

Once the ledger commit point is crossed, `commit` returns one of the two
success-side committed variants, never `CommitError`. `CommittedResult` carries
the last checksum-validated receipt plus exactly one v1 effect outcome after
structural completion; `CommittedRecoveryRequired` carries the known committed
identity/outcome when structural work remains. In `CommittedResult`,
`DeferredError` means a post-commit guard, corruption or
receipt-I/O problem prevented recording a trustworthy terminal effect state;
the returned receipt is explicitly the last validated projection. It is never
misreported as a pre-commit `CommitError`, and recovery/retry owns the next
step. V1 permits at most one aggregate executable effect.

`CommittedRecoveryRequired` covers failed structural work after the ledger
commit point: Complete-journal persistence, receipt publication, or older-
receipt reconciliation before an external effect may start. The known
operation, transaction and apply outcome are success-side facts, so it is never
`CommitError`. `receipt = Some` is legal only after that exact linked pair was
reread and checksum-validated; otherwise no receipt is fabricated. The journal
and objects remain for recovery, the typed stage and I/O/blocked/corrupt reason
are reported as a newly committed project, and the driver neither replans nor
starts an effect in that invocation.

Mutation JSON emits exactly one `CommandEnvelope`, and that envelope has
`recovery: Vec<RecoveryOutcome>`. A `RecoveredPriorTransaction(outcome)` is
appended in invocation order before the one allowed reload/replan; the fresh
attempt occupies the ordinary status/report/receipt/error fields. Outcomes are
never merged or dropped, and an earlier `pending_effects` snapshot is not
reinterpreted as final state. Implicit observationally clean recovery may be
omitted, while an explicitly requested recovery retains even an empty outcome.
Human output prints the same recovery entries before the requested result. If
authoritative recovery changes state again after the one replan, the command
returns `RecoveryBlocked` rather than looping and retains the first outcome in
the envelope. `plan.md` owns the exact public JSON field shapes, enum tags and
canonical encoding; this abstract fixes only the one-envelope/result semantics
and must not become a second wire specification.

`PreparedIdentityV1.object_manifest` is the exact, sorted result of one shared
`prepared_object_closure` traversal. Its roots are every `ObjectRef` reachable
from operations, ledger images, operation identity, preconditions, provenance,
conflict semantics and effect descriptors; traversal follows only declared
typed protocol references until raw byte objects. Every manifest member must
resolve through `PreparedChange.objects`, and extras are invalid. The durable
validator runs the same closure helper before trusting a journal or receipt.
Once R5 writing is active, every object reachable from a prospective ledger—
including bases, templates and render contexts—is promoted and synced in the
global content-addressed store before that ledger commits; a new ledger may
never depend only on a transaction/receipt-local copy. Earlier dark-R4 receipts
remain readable through the exact guarded source fallback above until a later
successful R5 commit promotes their reachable objects. This is a delivery
bridge, not a second authority or inline-base alternative. An absent ledger is
not the hash of invented empty bytes.

Every R5 GC cycle begins with an all-or-nothing promotion prepass over the full
object closure of **every retained receipt**, including dark-R4 objects used
only as file preimages or audit history and never referenced by the ledger. A
receipt-local copy becomes prunable only after its global copy is synced and
hash-verified. If any promotion fails, the cycle reports the failure and
deletes no local or global object; a later cycle retries the entire prepass.

A published receipt directory permanently contains both the immutable Complete
`journal.bin` and `receipt.bin`. The receipt checksum covers its
`complete_journal_checksum`, which must equal the sibling journal's record
checksum; transaction, generation and prepared identity must be byte-identical
in both records. Loading and recovery accept neither file alone. Retention is a
deterministic v1 dependency graph: root the latest 32 valid committed receipts
in each `PreparedKind` discriminant bucket (`Apply`, `Conflict`, `Finalise`,
`Abort`), the unique pending-origin receipt selected by full immutable
structural match, and every receipt with an executable `Deferred`, `Pending`,
`Running` or `Failed` effect. “Latest” sorts by generation and transaction ID,
never mtime; terminal `Succeeded`/`Superseded` effects add no root.
`last_operation` is not a transaction locator, and v1 has no administrative
pin API. Recursively retain every origin of a retained finalise/abort receipt;
a missing dependency is corruption. Only after that closure may receipts and
their now-unreachable objects be swept.

Directory creation, modes, contributors, effect state and exact before/after
images remain visible in reports and receipts. The storage protocol exists to
make interrupted transitions recoverable; it is not an incidental cache.

Every user-originated prepared change has
`operation_identity.invocation = Some(InvocationFingerprint)`;
recovery-only internal maintenance may use `None` only when it has no external
effect. Preparation first renders everything that cannot contain an operation
ID, determines the exact tools it will use, and records typed argv templates.
The sole placeholder form is `OperationLabel`; arbitrary string substitution
is forbidden. `OperationId` then hashes the typed request, snapshot, proposed
next ledger generation, semantic plan, tool identities and those templates.
Only afterward may preparation substitute the ID into a marker label or tool
argument. The full expanded argv hashes live in
`PreparationContextFingerprint`, which contributes to `TransactionId`, not
back to `OperationId`. No selected tool or argv template may change after the
operation ID is computed.

Ordinary apply and conflict include the closed `PlannedSubject` and
`LedgerIntent`; finalisation instead includes the complete frozen pending state
plus resolution images; abort includes the origin receipt identity plus guarded
restore targets. Each is a new operation. Ledger attribution and conflict
labels may embed it. Once every after-byte is exact, `TransactionId` hashes the
immutable prepared identity, including its complete object manifest; no
after-byte embeds that transaction ID. The exact identity is embedded in both
journal and receipt, while their mutable execution state has a separate record
checksum. `AppliedReceipt` is a derived API/report projection, not a second
durable authority. `ReceiptV1` never crosses the storage boundary as a command
result. `Report` is derived from `PreparedChange`, never stored beside it.
Absolute external paths and root identities live only in `CommitContext`;
durable preconditions contain opaque input IDs and hashes. `project_root` is
the loader's exact canonical-root device/inode. Commit compares it before
activation and writes it into the journal; recovery requires the live root,
runtime binding and journal identity to agree before trusting any path.
`ApplyOutcome` records only `Applied`, `Conflicted`, `Finalised`, or `Aborted`;
no-op has no receipt, and external effect failure remains orthogonal in effect
state and command status.

## 4. Planning and preparation

Planning says what the project should contain. Preparation proves that answer can become exact guarded
operations.

The planner consumes only a snapshot and `ResolvedMutation`. For a reconciliation
it resolves references and stable order, combines owner-local semantic
contributions, derives desired absence and presence, and advances the projected
view. For a one-shot or maintenance subject it applies that subject's closed
rule. For pending continue/abort it validates the pinned conflict and receipt;
after the pending gate, exactly one same-invocation `Deferred`, `Pending`,
first-attempt `Running`, or `Failed` effect emits only `RetryEffect`. A
different invocation never resurrects old external work and may instead commit
new logical state that supersedes it. Planning performs no live reads, writes,
printing, subprocesses or incidental policy. Commit plans carry semantic
`LedgerIntent`, not final ledger bytes. Preparation renders each deferred
template exactly once and is the sole owner of exact output/provenance and
ledger postimages.

Before the first project write, preparation performs every foreseeable fallible operation:

- render templates and package metadata;
- parse and splice POM, compose, properties, config, and source blocks;
- format generated or modified sources in scratch space;
- construct three-way merges and conflict markers;
- check confinement, collisions, ownership, case sensitivity, hashes, and ledger version;
- materialise the complete bytes of every create and replacement.

Prepared-kind validity is semantic, not merely structural. `Apply` has apply
semantics and no pending conflict before or after. `Conflict` has apply
semantics, preserves every successful top-level ledger table, and adds exactly
one pending candidate whose desired-present paths and semantic
`effect_intents` equal the prepared kind. Its prepared/receipt executable
effect vector is empty. `Finalise` requires that exact pending candidate and
its pinned conflict receipt, performs no file operation, promotes the entire
candidate with resolved current images, and materialises new executable effect
descriptors from the frozen intents and exact resolutions. `Abort` requires the
same origin, restores exactly every origin file preimage through guarded
forward operations, preserves the successful logical tables while clearing
pending state, and discards the intents without an effect descriptor.
Preparation, durable decode, commit and recovery all enforce this closed
matrix.

There is deliberately no lazy `Body::Computed(fn ...)` in a prepared operation. Lazy bodies defer failure
into commit, weaken exact preview, and make purity unenforceable. Eager `Artifact.contents` is the correct
existing direction.

`--pretend` follows the same `PlannedTransition` as execution. A `CommitPlan`
prepares and describes the same `PreparedBundle.change` as apply; an
`EffectRetryPlan` describes that exact retry directly. It has no separate
hand-written branch, performs no migration or legacy deletion, changes no
receipt state, and runs no post-commit effect. Its only uncertainty is a later
staleness check at commit/resume.

## 5. Execution and failure semantics

No portable filesystem provides a transaction across several files. jails promises a **recoverable commit**,
not instantaneous multi-file atomicity:

1. Acquire the project mutation lock.
2. Recover or refuse an earlier incomplete transaction.
3. Non-blockingly acquire the persistent effect lock before activation; every
   project commit is fenced from crossing its commit protocol while an external
   effect is running.
4. Recheck snapshot, runtime external bindings, ledger, absence conditions, and expected hashes. A prepared
   no-op performs these checks before it may return `NoOp`.
5. Durably persist and validate the complete journal, after-images, and required preimages before the first
   project mutation. Unvalidated staging is not an active transaction and can never precede a project write.
6. Apply deterministic operations through the mutation executor.
7. Persist the new ledger as the commit point.
8. Persist the Complete journal and linked receipt, validate the pair, then
   atomically publish the intact transaction directory as a receipt.
9. Reconcile older receipt effect guards against the now-current ledger and
   durably record any supersession. No external effect starts until journal
   completion, receipt publication and this structural reconciliation all
   succeed.
10. For a clean transition or conflict finalisation, attempt only its newly
   recorded idempotent post-commit effects and report them separately. A newly
   conflicted transition records semantic intents but no executable effect.

Recovery validates the canonical root, journal encoding and exact prepared identity, every referenced
object and the single-active-transaction invariant before trusting any operation. It then classifies the
ledger first and all affected live paths before making another project mutation. The durable phase matrix
is closed:

- `Prepared` plus every exact before-state is unactivated staging and is abandoned; `Prepared` plus **any**
  difference—including an exact after-image—blocks and is never promoted.
- `Active` plus `ledger_before` and only exact before/after path states rolls forward idempotently.
- `Active`, `LedgerCommitted`, or `Complete` plus `ledger_after` completes receipt/cleanup only; it does not
  rewrite project postimages.
- `LedgerCommitted`/`Complete` plus `ledger_before`, any ledger matching neither image, any unreadable image,
  or any path matching neither permitted state blocks without another project write.

Multiple active transactions or a corrupt journal/object likewise block without choosing an order or guessing.

Once the ledger records the transaction, recovery never rewrites project
postimages: it treats the journal as incomplete-transaction authority, completes
the durable Complete-journal/receipt pair and cleanup. Recovery is structural:
it never starts a subprocess. It may validate effect guards and atomically mark
an obsolete effect `Superseded`. While holding the project lock it must acquire
`effects.lock` non-blockingly before any receipt-state CAS; with that lock, an
orphaned `Running { attempt >= 2 }` becomes `Failed { code:
InterruptedTwice, .. }`, and no third automatic attempt exists. If the effect
lock is busy, recovery leaves the receipt untouched and reports its current
state. It reports every nonterminal state, including every `Running`. Only
`plan_all` for the same invocation may construct an `EffectRetryPlan`, and only
`resume_effect` executes it. A still-running attempt at least 2 means recovery
could not obtain the effect lock, blocks ordinary planning with `EffectBusy`,
and is never a subprocess plan. The ledger remains current logical authority; the
receipt is immutable prepared-history authority plus the one mutable
effect-state machine; objects supply only referenced bytes. Conflict receipts
have no executable effects, so structural recovery has no origin effect state
to rewrite. The persistent effect lock prevents duplicate
execution while the project lock is released around a long external call and
also fences every project commit as described above.
Repeating recovery is safe. Preimages support a new guarded conflict-abort transaction and audit;
default crash recovery never rolls a validated transaction back. A conflicted journal rolls forward to its prepared marker
files and `PendingConflict` state—it does not silently abort or finalise the conflict. Mutating commands run
recovery under the project lock; read-only and pretend commands report incomplete or blocked recovery state
without changing it.

The executor owns create, replace, delete, rename, copy, directory creation, permissions, and approved
mutating subprocesses. Create materialises a distinct synced publication inode and exposes it with an
absence-enforcing hard link; it never hard-links a mutable live file to an immutable receipt/object inode.
Replace and delete require prepared object identity. Read-only I/O need not be centralised there. Created
directories are monotonic structural shells and may remain empty after abort; automatic directory deletion
is deliberately outside this contract.

Formatters run on staged content and their bytes enter `FileOp`. Source registration is a semantic edit, not
a post-commit effect. Starting a service is a post-commit effect: it cannot join the filesystem transaction,
and its failure does not pretend committed files rolled back.

The only aggregate runtime descriptor is `ComposeReconcile`. It is emitted at
most once, only when `no_start == false` and either the complete managed
service map or committed compose output changes; owner-only/no-op transitions
emit none. A clean executable descriptor freezes the exact committed compose
pre/post documents, the complete prior and desired managed-service maps, and
the exact formerly managed names to stop.
`stop_services` is prior managed names minus *all* service names present in the
committed postimage, so a former managed name retained by the user as unmanaged
is not stopped. Execution
uses those immutable object bytes—never a fresh live compose document—as the
explicit `--file`, stops/removes only frozen removed names, then starts the
complete frozen desired managed set. It never invokes `down` or
`--remove-orphans`. Clean preparation derives the stop set from the committed
postimage; conflict preparation cannot do so and freezes only the exact
preimage plus semantic intent, with no executable descriptor. Finalisation
derives the postimage/stop set from the marker-free resolution. Clean
preparation and finalisation both require every stopped name to occur in the
frozen preimage. If this subset check fails during a pending conflict, the user
must abort and rerun the original command with `--no-start`; changing the flag
cannot mutate the frozen invocation. The executor never substitutes the
generated base or defers a predictable Docker failure, and durable decoding
repeats the invariant. Immediately before an attempt, the current ledger service
map and managed output must still match the descriptor; a later committed
transition makes it `Superseded`, while unrecorded live drift is typed
`EffectRunError::StaleInput`. Automatic and explicit compose mutation both use
the same effect lock and explicit project-root/process-input contract.

The external-call handoff releases the project lock but retains `effects.lock`.
After the subprocess returns, the runner reacquires the project lock
**blocking** while still holding the effect lock; competing mutators can hold
the project lock only until their nonblocking effect-lock attempt fails, so
this is the protocol's sole safe blocking lock acquisition. The runner rereads
the same receipt first. A checksum/generation/descriptor/expected-state
mismatch, or a changed project-root identity, causes no receipt rewrite and
returns typed corruption using the pre-call last-validated receipt projection.
Only the exact expected receipt under the exact root may revalidate ledger/live
guards. A guard mismatch at that point is out-of-protocol: compare-and-swap the
expected `Running` state to `Failed { code: Protocol, .. }`, report corruption,
and never bless the subprocess as success. Failure to persist and reread the
terminal state preserves the last validated projection and returns the typed
receipt-I/O channel; the runner never guesses whether a temporary rename won.

| Failure | Required meaning |
|---|---|
| Plan or prepare error | Project and machine state unchanged |
| Stale/refused commit attempt | No managed project leaf, human declaration, ledger, transaction, receipt, migration or content object is created or altered; executor-owned `.jails` coordination shells, fixed machine directories, persistent lock files and diagnostic lock contents may have been bootstrapped |
| Interrupted commit | Apply the complete phase/ledger/live-image matrix above: discard only `Prepared`/all-before staging; roll forward only eligible `Active` state; finish only after-ledger state; block every unreadable, unknown, neither-ledger or forbidden phase combination |
| Post-commit effect error after receipt publication | Last checksum-validated receipt identifies committed files and the terminal or deferred effect state |
| Post-ledger structural error | `CommittedRecoveryRequired` identifies the committed operation/transaction/outcome and exact unfinished stage; a receipt is present only after exact pair validation, and the journal/objects remain for structural recovery |
| Merge conflict | Journal-committed marker files plus explicit pending state; not an error after unrecorded mutation |

The coordination-shell exception applies only after an executor commit/resume
attempt. Loading, planning, preparation, pretend and read-only inspection
create none of that machine state. Tests assert these two promises separately;
“stale input” does not falsely promise that acquiring a persistent lock left no
filesystem trace.

A `PreparedKind::Conflict` is not a failed preparation. It contains the
marker-file operations and a `ledger_after` that preserves every successful
top-level table—`applied`, `one_shots`, `resources`, `outputs`, and `legacy`—
byte-for-value while adding `PendingConflict`. The complete prospective state
lives only in its candidate. It follows the same journal protocol through the
ledger commit point, returns `ApplyOutcome::Conflicted`, and does not run its
semantic effect intents; its executable effect vector is empty.

The ledger permits at most one project-wide pending conflict. While it exists, every ordinary mutation
refuses; only read-only inspection, finalisation, and guarded abort are allowed. The frozen transaction does
not regenerate, re-merge, accept a changed manifest, or overwrite a recorded path. While any path fails its
recorded desired-present rule, still equals its marker image, or still contains
its recorded conflict-marker form, finalisation refuses without mutation and
reports every unresolved path. Every conflict path has nonoptional prior and
desired bases and must exist marker-free; format 1 has no desired-absent
delete/modify marker protocol.

Once every recorded path is marker-free, every
`frozen_nonconflict_postimage` still equals its recorded exact image, and the pinned origin receipt still
has the expected checksum, an empty executable-effect vector and matching
semantic intents, rerun performs a journaled
**finalisation**, not another apply. It hashes the user's resolutions and promotes the complete frozen
`PendingLedgerState`—entities, one-shots, the global resources/outputs, and legacy rows—while constructing
resolved output-current images, retaining the prepared desired bases for future three-way merges, and
removing `PendingConflict`. For every resolved shared-format path it also parses
the frozen format and proves that each candidate-owned semantic slot occurs
exactly once with no collision; values may be user-resolved deltas, but slots
may not be deleted or duplicated while the candidate claims them. Promotion and
pending-state removal occur in the same ledger commit. Finalisation then
materialises new executable descriptors from the semantic intents and exact
resolved postimages; it never copies an origin effect. That invocation does not
also plan a changed manifest; a later invocation starts from the newly
finalised snapshot.

Aborting validates that same pinned conflict receipt and prepares a new
`PreparedKind::Abort { origin }` transition. It requires every affected
path—including clean postimages committed beside marker files—to equal its exact recorded transaction
postimage. Its forward operations restore file preimages and clear pending state while incrementing ledger
generation; the old receipt's prepared identity and file result remain
immutable history. Abort clears the pending semantic intents, while
finalisation derives a fresh effect only in its own receipt. If any path was edited,
abort refuses rather than discarding resolution or later user work.

## 6. Ledger and provenance

The ledger is a strict, versioned record of machine knowledge, not another intent manifest. Its schema
version is separate from jails and renderer versions. Loading is strict:

- absent means a new empty ledger;
- unreadable, corrupt, or unsupported-newer means an actionable error;
- an older supported schema parses without mutation;
- migration commits only with a mutating command or an explicit migration.

Conflict state is separate from successfully applied state:

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

struct LiveFileImage {
    sha256: ObjectId,
    len: u64,
    mode: FileMode,
}

struct StoredFileImage {
    object: ObjectRef,
    mode: FileMode,
}

struct PendingOutput {
    path: ProjectPath,
    contributors: BTreeSet<ResourceOwner>,
    current: PendingCurrent,
    base: StoredFileImage,
    renderer: RendererStamp,
}

enum PendingCurrent {
    Exact(LiveFileImage),
    ResolveFromLive,
}

struct PendingConflictPath {
    path: ProjectPath,
    prior_base: StoredFileImage,
    desired_base: StoredFileImage,
    marker_image: StoredFileImage,
    markers: MarkerTokens,
    hunk_count: u32,
}
```

A conflicted commit leaves all five successful top-level tables unchanged. The
pending record contains the complete candidate ledger state, including
entities, one-shots, one global resource table and desired output bases. A
pending output that needs human resolution has no invented current image;
finalisation learns its hash, length and mode from the marker-free live file.
Its path records and exact marker grammar make finalisation independent of the
current renderer. Its invocation fingerprint excludes presentation text and,
with frozen desired inputs, permits continue or abort only from the same
canonical command and unchanged human inputs. Frozen clean postimages protect
every file changed by the aggregate commit. Loading requires exactly one
retained receipt whose operation/generation, prepared conflict paths,
marker/clean postimages and candidate state structurally equal the pending
record, whose executable effect vector is empty and whose semantic intents
match. The resulting finalise/abort plan pins that receipt's
transaction and record checksum; discovery by operation/generation alone is
forbidden. This avoids embedding a self-referential transaction hash in ledger
bytes. That receipt retains project preimages.

The ledger has a monotonic generation and last `OperationId`. Applied entities
record declaration owners and complete specs; one canonical top-level
`ResourceRecord` exists per `ResourceKey`, and one canonical `OutputRecord`
exists per project path. Resource/output contributor sets connect those global
rows to entities and one-shots. They are never duplicated under each applied
row. `LegacyEntry` uses a closed machine `LegacySourcePath`, never a
project-mutation path that could target the current ledger. Explicit legacy
spec presence distinguishes a legitimate zero-argument intent from a path-only
legacy record; incomplete path-only rows remain `LegacyEntry` until explicit
adoption. Applied rows do not have a competing
conflict flag: the sole conflict authority is the optional project-wide
`PendingConflict`, while all last-successful top-level tables remain unchanged.

The authoritative old merge base is a content-addressed object beneath ledger
ownership, never inline bytes and never output regenerated by the current
binary. Every base/template/context object referenced by a committed ledger is
synced and hash-verified before that ledger commit. Without an exact base, a
renderer or relevant-context change causes safe refusal rather than a guessed merge.

`RendererStamp.context_object` contains one canonical `RendererContextV1`,
including the exact renderer and a closed subject:

```rust
enum RenderedSubjectContext {
    Entity { id: EntityId, spec: EntitySpec },
    OneShot { id: OneShotId, spec: OneShotSpec },
}
```

Recipe, capability and tool-feature renderers require the matching entity
identity/spec; a one-shot renderer requires the matching field, migration or
cases identity/spec; aggregate format renderers require no subject. The
renderer ID, subject discriminants and repeated identity fields must agree.
No renderer may omit its subject or smuggle that durable provenance through an
untyped template-binding map.

Migration preserves legacy input until the new ledger is durably committed.
Ambiguous package, ownership or field information is never invented. Exact
adoption selects one stable `LegacyKey` plus explicit manifest and intent. Plain
adoption is legal only when current bytes and mode exactly match a freshly
rendered candidate, so its `RendererStamp` is truthful; a mismatch requires the
guarded `--replace --force` route. Separate spec/path rows are retired only by
explicit key after the already-applied identity, spec, owner and output-path set
are proven equal—never by a heuristic join. Lossless cases migrate and lossy
cases stop with instructions. A read-only command never deletes legacy state. Production mutation dispatch switches from
schema 1 to schema 2 atomically only after every mutator supports V2; command-
by-command schema activation and dual write are both forbidden.

The ledger commits after project file operations because it describes the committed project. The journal
bridges the crash window; a second registry does not.

## 7. Reconciliation, removal, and conflict abort

App apply computes one desired graph. Manifest order is not dependency order; stable topological planning and
the projected project make dependent intents deterministic.

Reconciliation compares the exact recorded base, the user's current bytes, and newly prepared bytes. Clean
replacement, disjoint edits, conflicts, creations, and deletions are explicit cases. Retaining the original
base makes the same rules valid across renderer upgrades. After a clean merge,
the recorded current image becomes the actual merged live bytes, but the stored
base advances to the exact newly generated bytes—not the merge result. User
edits therefore remain a delta from the newest generator output instead of
being silently absorbed into the next base. Content and file mode are both
part of each image; incompatible concurrent mode changes refuse because marker
files cannot represent a mode conflict.

Removal is scoped forward planning. The active `ReconcileScope` relinquishes only its own `OwnerId` claim;
absence from one manifest or direct request cannot erase another manifest/config/direct claim:

1. Remove the active scope's owner claim from the entity.
2. Reject while any retained desired entity still has a typed dependency on
   the removed entity. There is no implicit cascade flag in this contract.
   Independently absent entities are ordered reverse-topologically and removed
   together.
3. Remove the entity row if and only if its own `OwnerId` set is empty; another resource owner does not keep a
   declaration alive.
4. Independently recompute each shared resource and output contributor set.
5. Retain each resource/output with any contributor; plan its semantic absence only when its own set empties.
6. Prepare the transition to the remaining desired state and delete only ownerless outputs whose guarded
   current image satisfies the reconciliation policy.

This replaces hand-maintained inverse algorithms. Forced removal of drifted generated content is an explicit
destructive choice, and its receipt says what was discarded.

Field one-shots are durable active overlays on their managed target, applied in
canonical `OneShotId` order every time that target is rendered. Their lifecycle
is `OneShotLifecycle::Field { target_coupled, append_only }`, with disjoint
resource sets, and their state is exactly `Active` or
`RetiredTargetRemoved`. Removing the target, after the normal retained-dependant check, retires each active field,
removes only its target-coupled contributions and preserves append-only
migrations/history. Recreating the target does not silently reactivate an old
field. An explicit identical field command may reactivate its target-coupled
resources without allocating a second forward migration; the same ID with a
different spec refuses. Migration and cases one-shots never use this retired
field state.

A cases import has a stable `CasesReceiptId` derived only from its canonical
`OneShotId::Cases { source }`, not mutable content, path output or transaction.
`destroy cases` selects exactly that one-shot either from an existing source or
from the printed receipt ID, so a moved/deleted external source never triggers
path guessing. A same-source import may refresh its source hash and output by
stored-base reconciliation, but its output path is immutable; a changed path
requires explicit destroy then generate.

`test --fast` is ordinary desired ownership of
`EntityId::ToolFeature(FastTest)` by `DirectCli`, not an imperative dependency
side channel. `remove fast-test [--force]` removes exactly that owner through
the same scoped, prerequisite and stored-base rules; it does not masquerade as
a capability or delete a drifted shared POM without the explicit force policy.

There is no universal receipt rollback command. Conflict abort is the one
supported inverse workflow because its pending ledger and pinned receipt freeze
the exact scope; it still becomes a new guarded forward transaction and refuses
after any affected postimage drift. Schema downgrade requires restoring the
whole project and `.jails` from one pre-migration VCS/backup snapshot.

## 8. Design principles and rejected abstractions

> **Model the output, not the process.**
>
> **Prepare completely; mutate once; recover explicitly.**
>
> **One authoritative mutation owner and one authoritative machine ledger.**
>
> **Adding one recipe or resource extends one typed model, not parallel switches and tables.**

Modules should hide stable decisions and ownership boundaries. File-format modules are good examples; thin
command coordinators are also legitimate. The successful subject-oriented splits show that “phase first” is
not universal. Cohesion and change isolation matter more than whether a filename is a noun or verb.

The system needs no inheritance, visitor, dependency-injection container, mutable object graph, or class per
operation. Rust enums, values, exhaustive matches, and narrow functions suffice. A Rust `match` is dispatch,
not “double dispatch”.

Explicitly rejected:

- calling a fallible partial merge a monoid, or calling preflight atomicity;
- lazy generated bodies inside commit;
- universal `revert(Project, Change)` for semantic removal;
- regenerating an old merge base with a new renderer;
- merging human manifests into machine ledger state;
- permanent compatibility ledgers or dual writes;
- runtime descriptors that duplicate compile-time recipe types;
- centralising every read merely because writes need one owner;
- line-count or role-stereotype targets used as substitutes for design evidence.

Duplication is cheaper than a speculative abstraction. Require an observed shared model, a narrow interface,
and behavioural tests. Widen an existing type when it fits; add a competing type only for genuinely different
semantics.

## 9. Verification contract

The architecture is complete only when tests demonstrate:

- golden output remains byte-stable unless a deliberate user-visible change updates it;
- planning and pretend leave project and machine state unchanged, including on legacy projects;
- planners perform no undeclared filesystem, environment, clock, or process reads outside snapshot loading;
- every managed-project mutation routes through the executor, including deletes, copies, directory operations,
  permissions, and managed mutating subprocesses; every other production writer has one named external or
  derived classification and ordering rule;
- describe and commit consume the same prepared operations; effect-retry
  describe and `resume_effect` consume the same guarded retry plan; a second
  clean apply is idempotent;
- mutation JSON emits one envelope whose ordered recovery outcomes survive the
  single reload/replan and whose remaining fields describe only the requested
  fresh result;
- duplicate identity, missing/incompatible references, reverse manifest order, and cycles are deterministic;
- scoped spec replacement permits a sole/agreed owner update, preserves every
  outside claim, refuses incompatible owner specs, and never creates a
  synthetic capability-prerequisite owner;
- human-config capability resources exist exactly for `DirectConfig`; manifest
  and CLI owners never copy declarations into `jails.toml`; the one first-V2
  exact bootstrap is ledger-only; complete exact DirectConfig resource closures
  and only the explicit current FastTest may bootstrap all-or-nothing; and `AdoptLayout` preserves a managed
  output's contributors/base/renderer while advancing only `current`;
- unreadable, corrupt, and newer ledger schemas fail closed;
- ledger-first loading reaches a pending conflict even when the POM,
  `jails.toml`, manifest or Java source contains markers; request syntax,
  manifest source and `Exact`/`ProjectedTransactionOutput`/`Absent` guards are
  all required before the frozen request is reused;
- zero-argument specs remain distinct from path-only legacy records;
- failure injection at each commit boundary obeys the complete phase matrix: only `Active` with the ledger
  before and exact before/after path states rolls forward, while every forbidden/unreadable combination
  blocks without further writes;
- a merely `Prepared` journal is discarded only when every image is before and is otherwise never activated;
  conflict abort is a new transaction that
  refuses after affected postimage drift, and shared contributions survive removal of one owner;
- stored-base reconciliation covers clean, disjoint, conflicting, added,
  deleted, mode-divergent and renderer-upgrade cases, including base advancement
  to desired rather than merged bytes;
- every captured/prepared/recovered image has a concrete mode; unset desired
  policy preserves replace mode or creates `0644`, executable output is
  explicit, and executor results are umask-independent;
- schema-1 migration reruns exact translation, targets only closed legacy
  machine deletes with empty contributors, consumes the old ledger through the
  ledger transition, and refuses before deleting any source when identity,
  listing or translated draft differs;
- exact legacy adoption requires the named key/manifest/intent and either an
  exact fresh-render match or explicit guarded replacement;
- active field overlays survive target re-render, target removal retires only
  target-coupled contributions, append-only history survives, and explicit
  identical reactivation creates no second migration; stable cases receipt
  selection works after source deletion; fast-test add/remove uses ordinary
  desired ownership;
- the prepared object manifest equals the exact typed reference closure with no
  missing or extra object; each retained receipt validates with its linked
  Complete journal before receipt-local fallback can be selected; the chosen
  global/lowest-receipt source is commit-guarded; R5 promotes every new ledger
  object globally before commit and promotes every retained receipt's full
  preimage/audit closure before any GC deletion, with failure deleting nothing;
  receipt-directory absence is distinct from present-empty; and v1 retention uses the four 32-receipt
  buckets, full pending-origin match, nonterminal effects and recursive
  finalise/abort dependencies before object collection;
- conflicted apply journal-commits marker images while retaining all five
  successful top-level tables unchanged and recording complete pending global
  ledger state, including one-shots;
- while any marker remains, any frozen clean postimage differs, or desired input changed, resolution leaves
  tree and ledger untouched; only complete recorded resolution journal-finalises the entire frozen pending
  ledger state, validates every candidate-owned shared-format slot exactly
  once, and materialises new effects from semantic intents and exact resolved
  postimages; zero/multiple or
  structurally mismatched origin receipts refuse before activation;
- conflict abort restores guarded file preimages at a new ledger generation, leaves only declared monotonic
  empty directories, clears intents, and refuses after any affected-file edit;
- structural recovery never runs a subprocess; only one same-invocation
  eligible effect becomes a retry plan, every `Running` state is reported,
  attempt 2 or greater becomes `InterruptedTwice` only under `effects.lock`,
  and lock contention leaves it unchanged and blocks ordinary planning;
- clean/finalised `ComposeReconcile` derives its stop set from frozen documents
  without `down` or orphan removal; conflict preparation freezes only preimage
  and intent, and a pending subset failure requires abort then rerun with
  `--no-start`;
- `effects.lock` fences every project commit from an executing effect; after a
  call the runner retains it while blocking for the project lock, rewrites no
  mismatched receipt/root, and records `Failed(Protocol)` only from the exact
  expected receipt when post-call guards are impossible;
- stale/refused executor attempts preserve all managed and transactional
  leaves while allowing only the documented coordination-shell/lock bootstrap;
  plan, prepare and pretend remain machine-state-free.

Static architecture ratchets inspect production code only. They inventory file/directory/permission mutation
APIs and process launch sites, require executor ownership or an explicit external/derived classification,
and permit zero unclassified production sites. Their numbers and ceilings live in
`tests/architecture.rs`, not here. A counter is evidence, not design, and may be withdrawn when its proxy no
longer measures the intended property.

The final test is conceptual: there is one route from `ResolvedMutation` to a
closed `PlannedTransition`, one commit branch from typed plan to exact prepared
operations to recoverable transition, and one bounded same-invocation
effect-retry branch to
`resume_effect`. Preview, apply, reconciliation, removal, doctor, and state
recording may observe different parts of those routes; they must not recreate
them.
