# abstract.md — mutation architecture contract

`plan.md` is the execution ledger: current status, delivery order, and acceptance tests. This document is
the stable architectural contract: which facts exist, who owns them, and which transitions are legal.

The contract describes the destination, not the current implementation. Section 2 is a short audited
reality check; `plan.md` is authoritative for live status. Exact binary tags, field ordering, magic bytes,
CLI JSON spelling, and fixture bytes have one authority only: the codecs and their golden tests. They must
not be copied into both design documents.

The numbered sections are retained for coarse orientation. Existing source comments that cite an old
step/part must be migrated to a named acceptance ID or stable semantic anchor when touched; a broad section
number does not preserve an obsolete ordered algorithm. A change that weakens an invariant here must update
this document and the test that enforces it in the same change.

## 0. The one sentence

jails turns a typed request and one immutable capture of a project into a completely prepared,
root-bound, recoverable transition—or into one guarded retry of an external effect already recorded by a
committed transition.

```text
syntax adapter
    -> CanonicalMutationRequest
    -> capture(ProjectHandle, request)
    -> LoadedMutation
    -> resolve claims and references
    -> PlannedTransition
       |-> Commit(PlannedCommit) -> prepare -> PreparedBundle -> describe | commit
       `-> RetryEffect(EffectRetryPlan) -> resume_effect
```

The arrows are ownership boundaries, not an invitation to make every stage a trait. For an existing
project, one concrete application service in the root crate drives them. `jails new` has a separate,
audited publication boundary because no project root exists yet. Command modules translate syntax and
render an outcome; they do not each reimplement either lifecycle.

The public conceptual API is:

```rust
capture(ProjectHandle, CanonicalMutationRequest, &CaptureCatalog)
    -> Result<LoadedMutation, StateLoadError>
resolve(LoadedMutation) -> Result<ResolvedMutation, PlanError>
plan(ResolvedMutation) -> Result<PlannedTransition, PlanError>
prepare(PlannedCommit, &PreparationServices) -> Result<PreparedBundle, PrepareError>
describe(&PreparedBundle) -> Report
commit(PreparedBundle) -> Result<CommitResult, CommitError>
resume_effect(EffectRetryPlan) -> Result<EffectRunResult, EffectRunError>
recover(ProjectHandle) -> Result<RecoveryOutcome, RecoveryError>
```

`PlannedTransition` is a closed choice between a structural commit and a retry of an effect already named
by a validated receipt. A commit plan is ordinary apply, conflict finalisation, or guarded conflict abort.
There is no generic rollback operation.

## 1. Scope and authority

This contract governs mutations within one project root: capability add/remove, artifact
generate/destroy, app apply/reconcile, one-shots such as field and cases, format, rename, adoption, and the
machine state under `.jails/`.

Authority is deliberately split:

1. `jails.toml`, `.jails/app.toml`, and direct command syntax express human desire. A surgical edit may
   preserve unrelated bytes, but these files are not machine state.
2. `.jails/ledger.toml` records the last successful logical state, ownership, provenance, and an optional
   pending conflict. It is not desired state and not an execution log.
3. One active journal is the authority for an incomplete structural transaction. A linked receipt is the
   immutable prepared history plus mutable effect state. Neither invents current desire.
4. Content-addressed objects have meaning only when reached from a validated ledger, journal, or retained
   receipt. Directory presence alone conveys no ownership.
5. Project files are shared. Ownership is attached to typed resources and outputs, not inferred from a
   pathname or from whichever command touched the file last.
6. Processes, containers, sockets, caches, and machine-level files are outside the project transaction.
   When a project mutation requires an external runtime effect, the structural commit records its exact
   descriptor first and the effect runner reconciles it separately.

Read-only commands may have smaller domain models, but a ledger-aware read uses the same strict machine
state capture. It never migrates, repairs, creates `.jails/`, deletes a legacy source, or recovers a
transaction as a side effect.

Legacy formats are compatibility inputs only. There is one production writer at a time, no dual write,
and no command-by-command schema activation.

## 2. Audited implementation reality

At baseline `7e54a606f99b` (2026-08-23), the workspace contains the root binary and nine library crates. It has
substantial R1–R5 foundations, but the production commands still use the earlier imperative writers.
Cargo dependencies on `jails-prepare` and `jails-commit` do not constitute integration.

| Area | What exists | What prevents a production claim |
|---|---|---|
| Typed protocol | IDs, requests, ownership, resources, snapshots, effects, schema-2 codecs; the ledger now carries `one_shots`, `resources` and `outputs` as canonical sets | Resolution is incomplete; `outputs` is written empty because these routes' bytes arrive without a `RendererStamp`; some legacy facts are invented |
| Capability desire bridge | 21/24 capabilities structurally encode on the current fixture; a test-only add-parity smoke test compared selected outputs for 20 rows in the audited environment | The smoke test permits install skips and covers only selected add outputs; test wiring is blocked, 2 rows remain prerequisite-unmeasured, and ownership/lifecycle semantics remain unproved |
| Persistent generator planning | Active `generate` now computes a write-free legacy `Change`; a dark test bridge matched projected generated-file bytes for 22 scenarios covering 21/33 persistent kinds | The value contains eager absolute artifacts plus selected deps/plugins, not complete identity/intent; imperative registration/state tails remain outside it, one-shots are separate, and the smoke test permits skips and compares files only |
| Snapshot/projection | A caller-declared read-set helper and test-only snapshot/projection shortcut exist | The shortcut mixes newly captured bytes with facts from a separately loaded `Project`, returns separable public values, and supplies no request, complete semantics, or authority; root/closure remain caller-chosen and projection remains independently constructible |
| Java source canonicalisation | Import ordering and blank-line tidying share one `jails-java::tidy` implementation across legacy and projected paths, with focused tests for both rules including the text-block case | Generic projection still selects it by `.java` suffix instead of receiving canonical bytes from a typed Java format owner |
| Preparation | Prepared values, file operations, sandbox, reconcile classifier, reports | Callers can supply mutually inconsistent base/projection/context; placeholder ledgers/invocation/effects cross the bundle; production has no caller |
| Commit/recovery | Lock, journal framing, structural apply, receipt skeleton, object helpers, recovery prototype | The prepared root is ignored by `commit`; recovery and object promotion are incomplete; effect execution is absent; production has no caller |
| Compatibility | A committed read-only `compat` facade | It parses only the old ledger as “current”, returns raw deletion paths, follows or suppresses I/O failures, and models unreadable state as a value |
| Reconciliation/provenance | B/L/N classifier, renderer stamps, pending-conflict and GC types | They are dark/test-only and are not representable in the current successful ledger end to end |
| Mutation inventory | Raw filesystem mutation outside the write layer is at a zero ratchet; process starters are classified | Existing command coordinators still perform multi-step legacy mutations; a primitive boundary is not transaction routing |

One stop-line defect remains for any activation:

- `prepare` accepts a `LoadedProject`, `CommitPlan`, snapshot, projection, and context assembled
  independently. The type system therefore permits a plan and projection from different captures.

The other is closed. A bundle prepared for project A could be passed to a lock for a same-shaped
project B: every path in a prepared operation is project-relative, so it would pass every
precondition against the wrong tree and write it. `commit` compares the prepared root against the
locked one before anything is activated, and a crash-suite test pins the refusal. The unit fixtures
that had been committing a plan rooted at `/srv/demo` into a temporary directory were only passing
because nothing compared.

Focused crate tests validate the reduced implementations. They do not prove this contract. The detailed
gap list and ordered repairs are in `plan.md`.

## 3. The internal model

The architecture has four value layers. Values may evolve in place; parallel “new” and “legacy” models
must not become permanent.

### 3.1 Semantic snapshot and runtime authority

`ProjectSnapshot` is immutable, deterministic, and relocatable. It uses `ProjectPath` throughout and does
not contain an absolute project root. It contains every semantic input a planner may use:

- captured machine state: `Fresh`, a canonical schema-2 ledger, or a typed legacy draft plus its source snapshot;
- captured declaration syntax and origins;
- resolved build kind, flavour, Java release, layers, package, and recipe facts;
- exact file images, absences, symlink classifications, modes, and sorted directory inventories;
- frozen template/tool/input descriptors and the bytes they selected;
- the verified object closure needed by the ledger;
- the bounded retained receipt inventory needed for conflict and effect decisions.

One `FactStore` owns parsed facts, their source paths, dependency edges, and invalidation. A projected edit
invalidates facts by path through that store. Format-specific modules parse and render; they do not keep
competing caches.

`SnapshotId` hashes the complete semantic capture—not merely the file read set. Changing a ledger
generation, declaration, absence, directory listing, selected object/receipt, template, tool identity, or
relevant external input changes the ID.

Snapshot ID proves semantic equality; it does not prove that two values came from the same capture and
never carries root authority. Lineage instead comes from ownership: one private, sealed planning-session
enum owns the request, semantic capture, authority, and—on the ready branch—its derived projection until
it emits a sealed plan.

Runtime authority is separate:

```rust
enum LoadedMutation {
    Ready {
        request: CanonicalMutationRequest,
        snapshot: Arc<ProjectSnapshot>,
        authority: CommitAuthority,
    },
    Pending {
        request: CanonicalMutationRequest,
        snapshot: PendingSnapshot,
        authority: CommitAuthority,
    },
    RecoveryRequired {
        request: CanonicalMutationRequest,
        recovery: RecoverySnapshot,
        project: BoundProject,
    },
}

struct CommitAuthority {
    project: BoundProject,
    external_inputs: BTreeMap<ExternalInputId, ExternalBinding>,
}

struct BoundProject {
    handle: ProjectHandle,
    identity: MachineRootBinding,
}
```

`MachineRootBinding` comes from a canonical, symlink-safe `ProjectHandle` and includes the OS identity
needed to detect replacement of the root. `ExternalBinding` maps an opaque captured input ID to the
runtime path/handle that commit may reopen. Absolute paths never enter semantic identities, reports,
ledgers, or renderer provenance.

Capture consumes its request. `CanonicalMutationRequest` determines the bounded declaration/template/
external-input closure, while `CaptureCatalog` supplies immutable built-in recipe/template metadata and
allowed external resolvers. The returned `LoadedMutation` owns that exact request. `resolve` takes no
second request and consumes the loaded value, so `ResolvedMutation` owns the same request and non-cloneable
authority instead of borrowing facts and later reconstructing either beside them.

Loading is ledger-first. With no pending conflict it returns a complete ordinary snapshot. With a pending
conflict it uses a deliberately smaller, parse-free bootstrap: marker-bearing human files and source are
not fed through ordinary parsers. Continue or abort is derived from the stored request fingerprint,
frozen desired-input guards, and the unique matching receipt.

An authenticated incomplete journal produces `LoadedMutation::RecoveryRequired`, not an ordinary snapshot.
A read-only command reports it without mutation. An executing driver recovers, records the recovery
outcome, and captures again. Schema-2 state plus legacy residue is legal only in that authenticated
post-commit recovery state; unexplained residue is corruption.

The loader—not a planner—computes input closure. A planner cannot add a live read later.

### 3.2 Request, references, and claims

CLI/manifests are syntax DTOs. They become one `CanonicalMutationRequest` before semantic work. Strings
that name entities are resolved once into typed identities; optional strings must not leak into planning as
“maybe resolved” references.

Recipe metadata declares identity shape, valid referents, prerequisites, required capabilities, default
package policy, and produced resource kinds. Reference validation creates a typed graph, rejects missing or
incompatible targets and cycles, then supplies stable topological order.

Entity/resource equality claims remain per authority until agreement is proved. The conceptual shape is:

```rust
type Claims<K, O, V> = BTreeMap<K, BTreeMap<O, V>>;
```

For one entity or resource key, every surviving claimant must claim the same value. Entity claims use
`OwnerId`; resource and whole-file claims use `ResourceOwner`. Only after agreement may the ledger compact
the result to `{ value, owners }`. Compacting early is wrong: it makes a simultaneous update by all owners
look like disagreement with the previous compacted value.

Shared semantic outputs are different: contributors intentionally supply different edits. They use
`OutputContributions = BTreeMap<OutputId, BTreeMap<ResourceOwner, SemanticContribution>>`; the one owning
format module validates and composes contributions in canonical order. Equality agreement must never be
applied to these contributions.

Omission has meaning only inside an explicit `ReconcileScope`. Silence from a direct request cannot remove
an app-manifest or config claim.

Conditional adoption/supersession belongs to typed recipe or migration metadata before resolution, never
inside resolved desired state. `AdoptIfPresent` names an owner, key, exact expected `ResourceValue`, and the
captured fact/provenance authority. Resolution against the same `FactStore` yields: no claim when absent; an
ordinary exact `DesiredResource` claim with no materialisation when the expected value is authoritatively
present; or refusal on value mismatch, unknown provenance, or ambiguous human ownership. A complete
schema-2 `ReconcileScope` handles normal supersession by withdrawing an omitted old claim while preserving
other owners. A resolved `DesiredChange` contains no key-only or “if present” instruction.

### 3.3 Desired state, resources, and projection

A plan describes the desired output, not the imperative process used to reach it:

```rust
struct DesiredChange {
    subject: PlannedSubject,
    claims: DesiredClaims,
    resources: Vec<DesiredResource>,
    files: Vec<DesiredFile>,
    edits: Vec<SemanticEdit>,
    absences: Vec<ManagedPath>,
    preconditions: Vec<SemanticPrecondition>,
    fact_delta: FactDelta,
}
```

`ResourceKey` is the unit of shared ownership: Maven coordinate, compose service, property key, marked
block, whole file, and other closed resource kinds. A path is not enough. Whole-file ownership and
semantic-contribution ownership cannot coexist at one path.

After owner agreement, a keyed materialisation obtains its payload from the single validated resource
value under that key. It must not accept an independent copy of that value: a change that claims A and
renders B must be unconstructible. Keyed edits carry only the key and rendering metadata that is not
resource authority.

Property values and introductory prose are deliberately different. `PropertyValue` is a private,
constructor-validated single-line value and participates in resource equality. An optional
`PropertyIntroduction` is private, canonical, single-line prose used only when an absent key is first
created. It is part of that prepared render's identity, but it is not an equality claim, ledger-owned
prose, or a deletion target. Existing prose is preserved; removing the last property owner removes the
property lines but not unmarked prose. If prose must be updated or retired, model a separately keyed marked
resource with explicit lifecycle rather than pretending an adjacent comment is owned.

`ProjectedProject` has no public constructor. The ready branch of the sealed planning session builds it
from the exact `Arc<ProjectSnapshot>` it owns. It does not accept an independent package/build/release/
flavour. The pending branch owns `PendingSnapshot + CommitAuthority`, admits only finalise or abort, and
does not manufacture an ordinary projection. Applying a desired change on the ready branch updates the
projected files, resource claims, output contributions, and `FactStore` together. Later planners observe
earlier projected changes without rereading disk.

Rendering remains deferred until preparation. When a projected edit already has known bytes, those bytes
are reparsed in stable `FactKind` order. `FactDelta` is a checked prediction against those parsed results,
never a second source of truth. Every replace—including a semantic edit—preserves the current projected
mode; a default mode is chosen only for a newly created path.

Canonicalisation is a pure format-owner operation. `jails-java` owns generated-source import ordering and
blank-line policy, including preservation of Java text blocks, but the typed Java materialiser invokes it
while producing desired or prepared source. Generic projection does not infer a language from a filename,
and commit never rewrites prepared bytes. The same rule and fixtures are shared by legacy-parity tests until
cutover, then the legacy call sites are removed.

Desired changes compose only through a fallible validator that detects duplicate paths, contradictory
absence/write claims, incompatible shared values, and semantic precondition conflicts. It is not a monoid.

### 3.4 Sealed planning and prepared state

The lineage boundary is a sealed value with private fields:

```rust
enum PlannedTransition {
    Commit(PlannedCommit),
    RetryEffect(EffectRetryPlan),
}

struct PlannedCommit {
    capture: PlannedCapture,
    plan: CommitPlan,
    authority: CommitAuthority,
}

enum PlannedCapture {
    Ready(Arc<ProjectSnapshot>),
    Pending(PendingSnapshot),
}
```

Only the sealed planning session constructs it. Before dropping transient state, `seal` performs a
branch-specific completeness proof: the ready branch replays the plan into a fresh projection and compares
the complete result; the pending branch reconstructs finalise/abort solely from its frozen pending snapshot
and stored guards. Only a successful comparison/validation emits `PlannedCommit`. `prepare` consumes that
sealed value and derives generation, ledger images, renderer/tool fingerprints, and invocation identity
from its one lineage. It does not accept correlated pieces as separate public parameters.

A prepared bundle is valid by construction:

```rust
struct PreparedBundle {
    identity: ValidatedPreparedIdentityV1,
    objects: PreparedObjectSet,
    authority: CommitAuthority,
}
```

`BoundProject` owns the captured `ProjectHandle` plus its `MachineRootBinding`; it is runtime-only. Commit
therefore has no second root argument that can disagree with the bundle, and it still reopens/checks the
canonical path to detect replacement beneath the handle.

Fields are private and the authority is non-`Clone`. Canonical decode is deliberately pure:

```rust
decode_canonical(bytes) -> UnvalidatedPreparedIdentityV1
validate(identity, &impl ObjectResolver)
    -> Result<(ValidatedPreparedIdentityV1, PreparedObjectSet), PreparedValidationError>
```

The two layers share one invariant/closure implementation without making `jails-protocol` perform I/O.
`PreparedValidationError` distinguishes resolver/source failure from a semantically invalid identity or
object closure; callers do not collapse either into absence.
Bundle construction uses an in-memory resolver; journal, receipt, recovery, and GC supply confined
resolvers. Durable decode can never manufacture runtime authority. `PreparedObjectSet` is exactly the
sorted transitive closure of every typed `ObjectRef` reachable from
operations, ledger images, preconditions, provenance, conflict semantics, migration inputs, and effect
descriptors. A missing object, extra object, hash/length mismatch, or unresolved typed reference is invalid.

Preparation resolves every semantic edit, renderer, merge, mode choice, tool run, refusal, and ledger
transition. It may use captured bytes in bounded scratch space. It does not read or mutate the live project.
The report is a pure projection of the prepared value, so preview and execution cannot describe different
operations.

Every planned/prepared mutation has a non-optional user invocation. Recovery reuses the journal's prepared
identity and never synthesises a planned mutation. No production-capable bundle may contain an invented absent ledger, missing user invocation, empty body
standing in for a referenced object, fabricated empty effect vector, unresolved reference, or runtime path
inside its durable identity.

`DesiredFile.mode = None` is policy before preparation: preserve the captured concrete mode on replace and
use the declared create default on create. Every prepared image has a concrete mode independent of umask.

## 4. Planning and application service

For existing project roots, the root crate owns one concrete `MutationDriver`; no new framework crate or
dependency-injection graph is needed. Its responsibilities are fixed:

1. accept a canonical request from a thin command adapter;
2. strictly capture once per planning attempt;
3. if capture reports recovery required, preview reports it unchanged; execution recovers and recaptures;
4. resolve all claims/references and plan against one projection;
5. prepare completely;
6. return the exact report for preview, without writes or recovery;
7. for execution, call `commit(PreparedBundle)`;
8. if a transaction raced in and commit recovered it, reload and replan at most once;
9. project commit/recovery/effect results into one human/JSON command outcome.

A second authoritative state change after that single replan blocks. There is no unbounded optimistic retry
loop and commit never replans while holding a lock.

Planning purity means no live filesystem, object-store, process, clock, random, environment, or network
access. Determinism is tested by running the same snapshot/request twice. Preparation may invoke a closed,
fingerprinted tool spec only over scratch copies of captured inputs. The tool runner clears the environment,
closes stdin, bounds output/time, owns the whole process group, and records exact executable/version/argv/
input identities.

A persistent recipe planner consumes the typed request through the ready session and emits the complete
desired contribution: outputs, resources, build edits, source registrations, human configuration, facts,
ownership, and ledger intent. It does not accept a separately loaded `Project`, return absolute paths, or
leave imperative post-plan tails. Field/cases/migration and other one-shots are distinct request variants
with their own closed allocation/source guards, not error-returning cases of the persistent planner.

Capture freezes the selected executable, version, runner/environment policy, and input identities.
Preparation revalidates that frozen tool and derives only invocation-specific argv and output identity. The
sole v1 nonliteral argument is the complete item
`OperationLabel { prefix: "jails-desired-", hex_chars: 12 }`, expanded from lowercase operation hex.

Read-only inspection and derived build processes do not need to pass through `MutationDriver`. Any command
that mutates an existing project root, its ledger, transaction storage, receipts, or effect state does.
Before `jails new` publishes, scratch writes are non-authoritative and belong only to the separate
`PublicationDriver`; its no-replace rename is the single transition that creates the new authority.

## 5. Commit, recovery, and effects

`commit(PreparedBundle)` owns sequencing. `LockedProject` and lock guards are internal
typestates, not public choices a caller can arrange incorrectly.

Before any transaction/object/project write, commit:

1. performs a read-only check that the canonical path still names the bundle's `BoundProject`;
2. acquires the mutation lock through the bound handle;
3. rechecks the root binding under that lock, before any recovery action;
4. classifies prior staging read-only and requires its journal root and closure to validate;
5. tries the effects lease nonblocking; contention releases the mutation lock with no authoritative change;
6. performs any required recovery while holding both fences and returns its outcome before using the new
   bundle;
7. validates the prepared identity and complete object closure;
8. rechecks every file, absence, directory, legacy source, ledger, object, receipt inventory, and external
   input precondition against the exact captured image.

A path already equal to an intended after-image is still stale before activation unless the prepared
operation explicitly had that image as its before-state. Coincidence is not permission to claim ownership.

The structural protocol is:

```text
mutation lock -> root/read-only staging classification -> try effects lease
     -> recover prior transaction or validate/recheck requested bundle
     -> stage objects -> Prepared journal -> Active journal
     -> guarded project operations -> promote/sync referenced objects
     -> ledger replacement (commit point) -> LedgerCommitted
     -> guarded Vec<LegacyRetirement>
     -> Complete journal + linked receipt -> publish receipt
     -> no effect: release both locks
     -> effect: queue/start receipt CAS while both locks are held
        -> release mutation lock only -> run -> reacquire mutation lock
        -> outcome receipt CAS -> release both locks
```

The ledger is last among semantic project changes. Before its durable replacement, failure is
pre-commit/recoverable from the journal. After it, the requested logical change is committed and the API
must return a success-side “recovery required” result rather than an error that invites replay.

Every production transaction has a present `ledger_after`. Only fresh/legacy bootstrap may have an absent
`ledger_before`; a true no-op creates no journal. If recovery makes any authoritative change before a
requested commit, commit returns `RecoveredPriorTransaction(outcome)` immediately without staging the stale
bundle. The driver records it, captures again, and replans once.

Recovery is one pure, exhaustive classifier over journal phase, ledger position, root identity, object
closure, and exact live mutation-target/retirement classifications. Captured non-target and external inputs
authorise initial activation but do not strand abandonment or roll-forward after a valid journal exists:

| Durable phase | Admissible state | Action |
|---|---|---|
| `Prepared` | ledger and every mutation/retirement target exactly before | abandon |
| `Active` | ledger before; project operations each exactly before/after; retirement still before | roll forward |
| `Active` | ledger after; project operations after; retirement before/after | finish retirement and publication |
| `LedgerCommitted` | ledger/project after; retirement before/after | finish retirement and publication |
| `Complete` | ledger/project/retirement all after | publish/verify only |
| any phase | root mismatch, unreadable/unknown image, or any other combination | block |

Recovery never guesses order from mtime, follows an unknown path, reparses desire, starts an effect, or
rolls back. A blocked journal is reclassified on each explicit retry, but its last reason is diagnostic
rather than authority.

External effects are a separate state machine under an effects lock. The structural receipt records the
descriptor and idempotency/guard identity before execution. Receipt updates use compare-and-swap; a newer
generation may supersede an obsolete effect. Recovery may change an orphaned `Running` attempt to a defined
retry/failure state, but structural recovery never executes it.

Lock order is exact. Every mutator of an existing project holds the mutation lock and tries `effects.lock`
nonblocking before creating a transaction or activating; this is required even when the new transaction
has no effect because it may supersede older work. Contention releases the mutation lock without changing
transaction or effect state. After receipt publication, an automatic effect handoff releases only the
mutation lock and retains the effect lease. The runner reacquires the mutation lock while still holding
that lease to record the result. No mutator waits for the effect lease while holding the mutation lock, so
the intentional effect→mutation reacquisition cannot deadlock.

The closed v1 transition table is:

| Prior state | Event | Next state |
|---|---|---|
| `Deferred` | queue first attempt | `Pending { attempt: 1 }` |
| `Pending { n }` | start the recorded attempt | `Running { n }` |
| `Running { n }` | process succeeds | `Succeeded { n }` |
| `Running { n }` | process fails with recorded reason | `Failed { n, reason }` |
| `Running { 1 }` | recovery proves orphan and `max_attempts >= 2` | `Pending { attempt: 2 }` |
| `Running { 1 }` | recovery proves orphan and `max_attempts = 1` | `Failed { 1, InterruptedAtLimit }` |
| `Running { n >= 2 }` | recovery proves orphan | `Failed { n, InterruptedTwice }` |
| retryable `Failed { n, reason }` | explicit retry | `Pending { attempt: n + 1 }` |
| `Deferred`, `Pending`, or `Failed` | guard/generation is obsolete | `Superseded` |

`Pending { n }` survives a crash before start and resumes the same attempt; it never increments merely
because the runner restarted. A lease-held `Running` state resolves only through process result or orphan
recovery, not concurrent supersession. `Succeeded` and `Superseded` are terminal. `Failed` is executable
only when its recorded policy permits another attempt; otherwise it is terminal.

Each descriptor stores `RetryPolicyV1 { max_attempts: NonZeroU32, retry_on }`. `retry_on` is a closed subset of
`NonZeroExit`, `Timeout`, `SpawnIo`, and `WaitIo`. Retry requires `n < max_attempts`; `InterruptedAtLimit`,
`InterruptedTwice`, protocol/corruption failures, attempt overflow, and a stale guard are never retryable.
Receipt/store I/O failure does not invent a state transition: return an I/O error and reload the
checksum-validated receipt.
Attempt zero and checked-add overflow are invalid. Every CAS pins the linked receipt checksum, effect ID/
descriptor, and exact expected prior state.

`EffectRetryPlan` is constructible only from a checksum-validated receipt in `Deferred`, `Pending { n }`,
or policy-retryable `Failed { n, reason }`. It pins that exact state and checksum, owns the bound runtime
authority, queues the first or next attempt when required, and starts exactly one pending attempt. It
rejects `Running`, terminal, policy-exhausted, corrupt, and overflowing states.

The v1 contract permits at most one aggregate executable effect per structural transaction. After receipt
publication, commit retains the effect lease and invokes the same recorded-effect runner used by
`resume_effect(EffectRetryPlan)` for the automatic first attempt. `EffectRetryPlan` itself owns its bound
project/external authority; resume has no second handle argument. Success,
nonzero exit, timeout, interruption, supersession, and receipt I/O are durable typed outcomes. This is
guarded at-least-once reconciliation, not a claim of universal exactly-once behavior for arbitrary external
systems.

An automatic effect failure occurs after the ledger commit point and is therefore a committed success-side
outcome carrying the last checksum-validated receipt, never a `CommitError` that invites structural replay.

Crash proof uses child-process termination and injected ordinary I/O failures at every named boundary. A
returned error or Rust unwinding is useful unit coverage but is not crash durability evidence.

## 6. Ledger, compatibility, objects, and provenance

The canonical successful schema-2 state has five logical tables:

1. applied entities with complete specs and owner sets;
2. one-shot receipts and lifecycle;
3. one global record per shared `ResourceKey`;
4. one `OutputRecord` per managed project path;
5. conservative `LegacyEntry` rows that have not been explicitly adopted.

It also has monotonic generation, last operation, writer/schema framing, and at most one complete pending
conflict. Successful tables describe the last successful state and remain unchanged by a conflicted commit;
the pending value contains the complete candidate state needed for finalisation.

Every output records contributors, exact current image, exact generated base, concrete mode, and a
`RendererStamp`. Provenance uses exact `ObjectRef` values and a closed renderer context: renderer/schema,
jails version, subject identity/spec where applicable, template origin and bytes, relevant-input digest,
and full tool identities. Relevant-input hashes are derived from the snapshot dependency graph, never
trusted as caller-supplied rows.

The old merge base is stored bytes, never output regenerated by a newer binary. All objects reachable from
a prospective ledger are hash-verified, promoted, and directory-synced before that ledger commits.

Compatibility capture has one fail-closed shape:

```rust
fn capture_machine_state(root: &ProjectHandle)
    -> Result<CapturedMachineState, StateLoadError>;

enum CapturedMachineState {
    Fresh { machine_root: MachineRootPresence },
    Legacy {
        ledger: Option<LegacyLedgerV1>,
        translated: LedgerV2Draft,
        sources: LegacySnapshot,
    },
    Current {
        ledger: LedgerV2,
        store: MachineStoreSnapshot,
    },
}
```

The reader classifies the envelope/schema before parsing, parses once, and fails closed on malformed,
unsupported-newer, unreadable, symlinked, or ambiguous recognised inputs. `LegacySnapshot` contains closed
`LegacySourcePath` identities, exact bytes/hash/length/mode, and guarded sorted directory inventories—never
raw `PathBuf` deletion targets. Only `NotFound` is absence. Unknown entries are never nominated for cleanup.

Capture translates purely and mutates nothing. Preparation reruns the pure translator over the sealed
`LegacySnapshot` bytes already in `ProjectSnapshot`—never by reopening live paths—and proves the typed
draft. It records each recognised legacy sidecar/directory as guarded post-ledger retirement in the same
journaled transaction. The schema-1 ledger is replaced at the commit point; sidecars remain at their
original paths until schema 2 is durable. An interruption after that point yields authenticated
`RecoveryRequired`; schema 2 plus otherwise unexplained residue is corruption, not another migration.
Ambiguous package, spec, path, or ownership remains `LegacyEntry`; migration never invents a plausible
owner.

Journal and receipt decoding use the shared object-closure validator. A published receipt is accepted only
as a linked Complete-journal/receipt pair with matching transaction, generation, prepared identity, and
checksum witness. Receipt capture roots the latest 32 valid receipts in each prepared-kind bucket ordered
by `(generation, transaction_id)`, the unique pending origin, every executable/nonterminal-effect receipt
(including policy-retryable `Failed` below its maximum), and recursive
finalise/abort origins. The resolved union may contain at most 4,096 receipts in v1; overflow fails closed
rather than dropping a root. GC uses that same selection, then promotes and syncs every retained
object before deleting anything. One failure means no sweep.

## 7. Reconciliation, removal, and conflicts

For each path, reconciliation compares exact recorded base `B`, live image `L`, and newly desired image
`N`. File action and ledger/output transition are separate results; a “no file operation” must not hide
“drop ownership” or “advance base”.

Without a prior output:

| Live | Desired | Result |
|---|---|---|
| absent | absent | no file and no ownership |
| absent | present | create and record ownership |
| present | absent | preserve as unowned |
| present | present | refuse; explicit adoption is required even when bytes match |

With a prior output:

| Relation | Result |
|---|---|
| `L = B = N` | no file operation; retain output |
| `L = B`, `N != B` | replace with `N`; advance base/current to `N` |
| `L != B`, `N = B` | keep user bytes; keep base `B`; record exact current |
| `L = N`, both differ from `B` | no file operation; advance base/current to `N` |
| all differ | bounded three-way merge; on clean merge current is merged bytes and base advances to exact `N` |
| desired absent, `L = B` | delete and remove output ownership |
| desired absent, `L != B` | refuse unless an explicit destructive policy is prepared |
| live absent, desired present | refuse unless explicit replace/recreate is prepared |

Mode follows its own three-way rule. If both live and desired changed the base mode differently, refuse;
marker text cannot resolve a permission conflict.

Removal is forward planning against claims that remain:

1. remove only the active scope's claim;
2. reject removal while a retained typed reference requires the entity;
3. delete an entity only when its entity-owner set is empty;
4. recompute shared resource/output contributors independently;
5. remove a contribution only when its contributor set empties;
6. reconcile exact bases and prepare guarded deletes.

There is no hand-maintained inverse generator and no receipt-as-uninstall model.

A conflicted merge commits marker bytes plus a complete frozen candidate and a receipt with no executable
effect. Finalisation is a new forward transaction that validates marker removal, clean postimages, desired
input guards, and the unique origin receipt before promoting the candidate and deriving a fresh effect.
Abort is also a new guarded forward transaction restoring recorded preimages and clearing pending state; it
refuses after affected postimage drift.

## 8. Design principles and rejected abstractions

The governing rules are:

> Model the output, not the process.
>
> Capture once, prepare completely, mutate once, recover explicitly.
>
> One authoritative machine ledger, one mutation driver, one object-closure algorithm.
>
> Make invalid lineage, ownership, and phase combinations unrepresentable.

Stable domain decisions belong in the lowest crate that can own them without I/O. Format modules own one
parser/renderer each. The root application service owns orchestration. The commit crate owns mutation,
durability, and locks. Cohesion and one-way dependencies matter more than noun/verb filenames.

Explicitly rejected:

- separate public base, projection, context, and plan parameters that merely promise to agree;
- a second machine-state facade beside `LoadedMutation`;
- public half-validated structs whose callers can assemble impossible combinations;
- a trait or class per pipeline stage without two real implementations;
- lazy reads or rendering during commit;
- regenerating an old merge base with current code;
- permanent schema adapters, dual writes, or heuristic legacy adoption;
- a universal `revert(Change)` or receipt-driven uninstall;
- treating a path, hash, or empty string as a typed identity;
- centralising every read merely because all writes need one owner;
- declaring a phase shipped because types or isolated unit tests exist;
- line counts, crate counts, or zero raw `fs::write` sites as substitutes for end-to-end authority.

## 9. Verification contract

The implementation is conformant only when all of these are enforced at constructors/decoders and at the
integration boundary:

- the same snapshot/request yields byte-identical planned and prepared identities;
- a mismatched snapshot/projection cannot be constructed through public APIs;
- replaying a complete plan into a fresh projection reproduces the planner's projected paths/body
  descriptors, known bytes, modes, deferred render nodes, facts, claims, ledger intent, and base snapshot;
- no public API can rebind an A-prepared bundle to B, and replacing/moving A after capture refuses before
  any store, transaction, ledger, or project write;
- every captured file, absence, directory, object, receipt, legacy source, external input, and root binding
  is rechecked under the mutation lock;
- object closure rejects missing members, extras, wrong length/hash, and invalid typed references;
- the complete ownership-claim and B/L/N tables have positive and refusal tests;
- every recovery matrix cell is table-tested and crash-tested at real process boundaries;
- effects are fenced, CAS-updated, resumable, and never run by structural recovery;
- a read-only compatibility pass is byte-for-byte non-mutating under success and every error;
- no existing-project mutation adapter bypasses `MutationDriver`, the sole audited absent-destination
  `PublicationDriver` boundary is no-replace, and no legacy writer is reachable after cutover;
- proof applications rebuild from declarations through the public CLI on a capable host.

Environment-constrained or skipped external tests are reported as such. They are never counted as passed.
The named acceptance IDs, commands, and delivery order live in `plan.md`.
