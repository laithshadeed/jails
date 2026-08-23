//! `prepare`: the one place semantic desire becomes an exact transition.
//!
//! plan.md §R3.2 gives a closed fourteen-step algorithm. What is implemented
//! here is every step that can be decided from values that already exist —
//! reapplying the changes, guarding the generation, materialising deferred
//! renders, diffing against the snapshot, deriving parents, and freezing the
//! identity. The steps that need machinery a later phase owns are named
//! rather than approximated: R3.3 supplies the formatter sandbox, R5 supplies
//! the three-way merge, and until they exist a plan that would need one
//! refuses instead of guessing.
//!
//! ## Why reapplying is not redundant
//!
//! Step 1 replays every `DesiredChange` onto a *fresh* projection and
//! requires the result to equal the `LedgerIntent` the planner computed. The
//! planner already walked those changes once. Doing it again catches the case
//! that matters: a planner whose intent and whose changes have drifted apart
//! would otherwise write files from one and a store from the other, and the
//! store is what the next run reads.
//!
//! ## Why the diff is against the snapshot, not against disk
//!
//! Because the snapshot is what the plan was made from. Diffing against disk
//! would silently absorb an edit made after capture — and absorbing it is
//! exactly what the guarded preimage exists to refuse.

use crate::Result;
use crate::migration::LegacyMigrationIdentity;
use crate::operation::{ApplySemantics, OperationIdentityV1, OperationSemanticsV1};
use crate::prepare::{
    DirectoryOp, FileOp, GuardedImage, OperationTarget, PreparedChange, PreparedKind,
};
use crate::tool::{OperationContextFingerprint, PreparationContextFingerprint};
use jails_project::projection::{ProjectedEntry, ProjectedProject};
use jails_protocol::bootstrap::LoadedProject;
use jails_protocol::change::DesiredChange;
use jails_protocol::conflict::{FileImage, FileMode};
use jails_protocol::envelope::LedgerV2;
use jails_protocol::identity::{ObjectId, ObjectRef, ProjectPath, TemplateKey};
use jails_protocol::plan::{LedgerIntent, PlannedSubject};
use jails_protocol::render::{DesiredBody, TemplateValue};
use jails_protocol::resource::ResourceOwner;
use jails_protocol::snapshot::{Captured, ProjectSnapshot, ReadSet, TemplateStore};
use jails_protocol::transition::{AbortPlan, CommitPlan, FinalisationPlan};
use jails_support::codec::sha256;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The machine-side inputs preparation needs and the plan does not carry.
/// The store as this run read it.
///
/// Two halves that must agree: the *file* image the commit will guard against
/// under the lock, and the value that was decoded from it. An absent file with
/// a decoded store, or the reverse, would let a plan be written against a
/// store nobody saw.
#[derive(Clone, Debug)]
pub struct ObservedStore {
    pub image: FileImage,
    pub ledger: Option<LedgerV2>,
}

impl Default for ObservedStore {
    /// A project with no store yet, which is where every project starts.
    fn default() -> Self {
        Self {
            image: FileImage::Absent,
            ledger: None,
        }
    }
}

impl ObservedStore {
    /// The generation a plan computed against this store must claim.
    pub fn generation(&self) -> u64 {
        self.ledger.as_ref().map_or(0, |ledger| ledger.generation)
    }

    fn validate(&self) -> Result<()> {
        match (&self.image, &self.ledger) {
            (FileImage::Absent, None) | (FileImage::Present { .. }, Some(_)) => Ok(()),
            _ => Err(
                "the observed store's file and its decoded value disagree about whether it \
                 exists"
                    .to_string(),
            ),
        }
    }
}

pub struct PreparationContext {
    /// Everything the plan was allowed to read.
    pub read_set: ReadSet,
    /// Every template this run may render, resolved once.
    pub templates: TemplateStore,
    /// The generation the ledger was observed at.
    pub observed_generation: u64,
    /// The store this plan was computed against, and the file it came from.
    pub observed_store: ObservedStore,
    /// The tools this operation intends to run. Frozen at step 5; empty when
    /// no formatter or merge is selected.
    pub operation_context: OperationContextFingerprint,
    /// The tools preparation actually ran, with their exact arguments.
    pub preparation: PreparationContextFingerprint,
}

/// The prepared change plus the runtime-only bindings a commit needs.
///
/// They are separate values because one is durable and the other is not: an
/// absolute path is meaningless on another machine and must never reach a
/// journal, a receipt or a report.
#[derive(Debug)]
pub struct PreparedBundle {
    pub change: PreparedChange,
    pub root: jails_protocol::snapshot::CanonicalRoot,
}

/// Run the preparation algorithm.
pub fn prepare(
    project: &LoadedProject,
    plan: CommitPlan,
    base: Arc<ProjectSnapshot>,
    projection: ProjectedProject,
    context: PreparationContext,
) -> Result<PreparedBundle> {
    // The pairing check from R2.3, applied once more at the boundary: a plan
    // may have been built before the project was reloaded.
    let plan = plan.for_bootstrap(project)?;
    match plan {
        CommitPlan::Apply(set) => apply(base, projection, set, context),
        CommitPlan::Finalise(finalisation) => finalise(base, finalisation, context),
        CommitPlan::Abort(abort) => abort_plan(base, abort, context),
    }
}

fn apply(
    base: Arc<ProjectSnapshot>,
    mut projection: ProjectedProject,
    set: jails_protocol::plan::DesiredChangeSet,
    context: PreparationContext,
) -> Result<PreparedBundle> {
    set.validate()?;

    // Step 2. The generation the plan was computed against must still be the
    // one observed, or this plan describes a store that has moved on.
    if set.ledger_intent.generation_before != context.observed_generation {
        return Err(format!(
            "this plan was computed against generation {}, and the store is at {}.\n       fix: \
             replan; applying it would write a store that never existed.",
            set.ledger_intent.generation_before, context.observed_generation
        ));
    }

    // Step 1. Replay onto a fresh projection and require the same resources.
    for change in &set.ordered {
        projection.advance(change)?;
    }
    require_intent_matches(&projection, &set)?;

    // Step 3. Materialise every deferred render from the frozen template
    // bytes. After this no `DesiredBody::Render` survives.
    let rendered = materialise(&projection, &set.ordered, &context.templates)?;

    // Step 8. Diff the final projection against the snapshot images.
    let (operations, objects) = diff(&base, &projection, &rendered)?;

    // Step 9. Parents for creates only, stopping at the machine root.
    let directories = parents(&base, &operations)?;

    let semantics = OperationSemanticsV1::Apply(Box::new(ApplySemantics {
        subject: set.subject.clone(),
        ledger_intent: set.ledger_intent.clone(),
        migration: None,
    }));
    assemble(
        base,
        semantics,
        PreparedKind::Apply,
        operations,
        directories,
        objects,
        context,
    )
}

fn finalise(
    base: Arc<ProjectSnapshot>,
    plan: FinalisationPlan,
    context: PreparationContext,
) -> Result<PreparedBundle> {
    plan.validate()?;
    // §R3.2 step 9: finalisation has no file or directory operation. The
    // resolved bytes are already on disk — the user put them there — and
    // rewriting them would discard the resolution.
    let semantics = OperationSemanticsV1::Finalise {
        origin: plan.origin.operation,
        origin_transaction: plan.origin.transaction,
        pending: plan.origin.pending,
        resolutions: plan.resolutions.clone(),
    };
    assemble(
        base,
        semantics,
        PreparedKind::Finalise {
            origin: plan.origin.operation,
        },
        Vec::new(),
        Vec::new(),
        BTreeMap::new(),
        context,
    )
}

fn abort_plan(
    base: Arc<ProjectSnapshot>,
    plan: AbortPlan,
    context: PreparationContext,
) -> Result<PreparedBundle> {
    plan.validate()?;
    let mut operations = Vec::new();
    let mut objects = BTreeMap::new();
    for restore in &plan.restores {
        // `guarded_from` is what the abort expects to find. Restoring without
        // checking it would overwrite an edit made after the conflict froze.
        let op = match (restore.guarded_from, restore.restore_to) {
            (FileImage::Present { object, mode }, FileImage::Absent) => FileOp::Delete {
                path: OperationTarget::Project(restore.path.clone()),
                before: GuardedImage { object, mode },
                contributors: BTreeSet::new(),
            },
            (
                FileImage::Present { object, mode },
                FileImage::Present {
                    object: after,
                    mode: after_mode,
                },
            ) => {
                objects.insert(after.id, Arc::from(Vec::new().into_boxed_slice()));
                FileOp::Replace {
                    path: OperationTarget::Project(restore.path.clone()),
                    before: GuardedImage { object, mode },
                    after,
                    mode: after_mode,
                    contributors: BTreeSet::new(),
                }
            }
            (FileImage::Absent, FileImage::Present { object, mode }) => FileOp::Create {
                path: OperationTarget::Project(restore.path.clone()),
                after: object,
                mode,
                contributors: BTreeSet::new(),
            },
            (FileImage::Absent, FileImage::Absent) => {
                return Err(format!(
                    "{} is restored from absence to absence, which is not a restore",
                    restore.path
                ));
            }
        };
        operations.push(op);
    }
    operations.sort_by(|a, b| a.target().cmp(b.target()));
    let semantics = OperationSemanticsV1::Abort {
        origin: plan.origin.operation,
        origin_transaction: plan.origin.transaction,
        restores: plan.restores.clone(),
    };
    assemble(
        base,
        semantics,
        PreparedKind::Abort {
            origin: plan.origin.operation,
        },
        operations,
        Vec::new(),
        objects,
        context,
    )
}

/// A planner whose intent and whose changes disagree would write files from
/// one and a store from the other, and the store is what the next run reads.
fn require_intent_matches(
    projection: &ProjectedProject,
    set: &jails_protocol::plan::DesiredChangeSet,
) -> Result<()> {
    let declared: BTreeMap<_, _> = set
        .ledger_intent
        .resources_after
        .iter()
        .map(|resource| (resource.key.clone(), (&resource.value, &resource.owners)))
        .collect();
    for (key, projected) in projection.resources() {
        match declared.get(key) {
            Some((value, owners)) if *value == &projected.value && *owners == &projected.owners => {
            }
            Some(_) => {
                return Err(format!(
                    "the changes and the ledger intent disagree about {key:?}.\n       fix: they \
                     are computed from one plan; a difference means the files and the store \
                     would describe different projects."
                ));
            }
            None => {
                return Err(format!(
                    "the changes claim {key:?} and the ledger intent does not record it"
                ));
            }
        }
    }
    for key in declared.keys() {
        if !projection.resources().contains_key(key) {
            return Err(format!(
                "the ledger intent records {key:?} and no change claims it"
            ));
        }
    }
    Ok(())
}

/// Render every deferred body from the frozen template bytes, once.
fn materialise(
    projection: &ProjectedProject,
    changes: &[DesiredChange],
    templates: &TemplateStore,
) -> Result<BTreeMap<ProjectPath, Vec<u8>>> {
    let mut out = BTreeMap::new();
    for change in changes {
        for file in &change.files {
            let DesiredBody::Render { template, bindings } = &file.body else {
                continue;
            };
            // A later change may have replaced this path outright, in which
            // case the render is dead and rendering it would be work whose
            // result nothing reads.
            if !matches!(
                projection.entry(&file.path),
                Some(ProjectedEntry::Deferred { .. })
            ) {
                continue;
            }
            let resolved = templates.resolve(template)?;
            let supplied: Vec<(String, String)> = bindings
                .keys()
                .map(|key| Ok((key.as_str().to_string(), flat(bindings.get(key), key)?)))
                .collect::<Result<Vec<_>>>()?;
            let borrowed: Vec<(&str, &str)> = supplied
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect();
            let body = jails_java::template::try_render(&*resolved.source, &borrowed)
                .map_err(|error| format!("{template}: {error}"))?;
            out.insert(file.path.clone(), body.into_bytes());
        }
    }
    Ok(out)
}

/// One binding as text.
///
/// An `Ordered` value has no single rendering — a template is substitution
/// only, with no loops, so a list has to have been rendered by whoever knew
/// what separator it wanted. Reaching here means it was not.
fn flat(value: Option<&TemplateValue>, key: &TemplateKey) -> Result<String> {
    Ok(match value {
        Some(TemplateValue::Text(text)) => text.clone(),
        Some(TemplateValue::Name(name)) => name.as_str().to_string(),
        Some(TemplateValue::Package(package)) => package.as_str().to_string(),
        Some(TemplateValue::JavaType(java_type)) => java_type.to_string(),
        Some(TemplateValue::Boolean(value)) => value.to_string(),
        Some(TemplateValue::Ordered(_)) => {
            return Err(format!(
                "`{key}` is an ordered value, and a template is substitution only.\n       fix: \
                 render the list where its separator is known and bind the result."
            ));
        }
        None => return Err(format!("`{key}` is bound to nothing")),
    })
}

/// The bodies a prepared change will write, keyed by content address.
pub type ObjectBundle = BTreeMap<ObjectId, Arc<[u8]>>;

/// Step 8: every path whose projected state differs from what was captured.
fn diff(
    base: &ProjectSnapshot,
    projection: &ProjectedProject,
    rendered: &BTreeMap<ProjectPath, Vec<u8>>,
) -> Result<(Vec<FileOp>, ObjectBundle)> {
    let mut operations = Vec::new();
    let mut objects: BTreeMap<ObjectId, Arc<[u8]>> = BTreeMap::new();

    for (path, entry) in projection.overlay() {
        let before = base.read(path)?;
        let contributors = projection.contributors(path);
        let after: Option<(Vec<u8>, FileMode)> = match entry {
            ProjectedEntry::File(file) => Some((file.bytes.to_vec(), file.mode)),
            ProjectedEntry::Deferred { .. } => {
                let body = rendered.get(path).ok_or_else(|| {
                    format!("`{path}` is still a deferred render after materialisation")
                })?;
                Some((body.clone(), default_mode()))
            }
            ProjectedEntry::Deleted => None,
        };

        match (before, after) {
            (Captured::Absent, None) => {
                // A create that a later change deleted collapses to nothing,
                // which is §R3.2 step 8's rule and not an omission.
            }
            (Captured::Absent, Some((body, mode))) => {
                let object = intern(&mut objects, body);
                operations.push(FileOp::Create {
                    path: OperationTarget::Project(path.clone()),
                    after: object,
                    mode,
                    contributors,
                });
            }
            (Captured::Present(file), None) => operations.push(FileOp::Delete {
                path: OperationTarget::Project(path.clone()),
                before: GuardedImage {
                    object: ObjectRef::new(file.sha256, file.len),
                    mode: file.mode,
                },
                contributors,
            }),
            (Captured::Present(file), Some((body, mode))) => {
                let object = intern(&mut objects, body);
                // Equal bytes *and* mode emit no operation. A file with the
                // right bytes and the wrong mode is not the file that was
                // meant, so mode is part of the comparison.
                if object.id == file.sha256 && mode == file.mode {
                    continue;
                }
                // An *owned output* -- a file some entity claims, as opposed
                // to a shared file this change merely edits -- goes through
                // R5.3's reconciliation, which is where "jails did not write
                // this" is decided. Without this the preparation happily
                // planned a replace over a file somebody had written by hand,
                // and the receipt recorded the entity as its contributor.
                //
                // `prior` is `None` until applied outputs are read back from
                // the ledger. That makes an update to jails' own earlier
                // output refuse rather than replace, which is the safe
                // direction to be wrong in while the plumbing lands.
                if !contributors.is_empty() {
                    let live = FileImage::Present {
                        object: ObjectRef::new(file.sha256, file.len),
                        mode: file.mode,
                    };
                    let desired = FileImage::Present { object, mode };
                    match crate::reconcile::reconcile(path, None, live, desired)? {
                        crate::reconcile::Decision::Refuse(why) => return Err(why),
                        crate::reconcile::Decision::Nothing => continue,
                        _ => {}
                    }
                }
                operations.push(FileOp::Replace {
                    path: OperationTarget::Project(path.clone()),
                    before: GuardedImage {
                        object: ObjectRef::new(file.sha256, file.len),
                        mode: file.mode,
                    },
                    after: object,
                    mode,
                    contributors,
                });
            }
        }
    }
    operations.sort_by(|a, b| a.target().cmp(b.target()));
    Ok((operations, objects))
}

/// Whether a computed store says anything the observed one did not.
///
/// Only the rows are compared. `written_by`, `generation` and
/// `last_operation` change on every commit by construction, so including them
/// would make every store differ from itself.
fn unchanged(store: &LedgerV2, observed: &Option<LedgerV2>) -> bool {
    let Some(observed) = observed else {
        return store.applied.is_empty()
            && store.one_shots.is_empty()
            && store.resources.is_empty()
            && store.outputs.is_empty()
            && store.legacy.is_empty()
            && store.pending_conflict.is_none();
    };
    store.applied == observed.applied
        && store.one_shots == observed.one_shots
        && store.resources == observed.resources
        && store.outputs == observed.outputs
        && store.legacy == observed.legacy
        && store.pending_conflict == observed.pending_conflict
}

/// The store this transition will leave behind.
///
/// An *update* of what was observed, never a fresh construction: a row this
/// request says nothing about -- another capability's dependency, an entity it
/// has never heard of -- survives untouched. Rebuilding the store from one
/// request's intent would quietly delete everything the request did not
/// mention, which is the opposite of what a scope means.
///
/// `outputs` is deliberately not written here. §R1.4's `OutputRecord` carries
/// a `RendererStamp`, and this route's bytes arrive already rendered by a
/// recipe that never produced one. An output row with an invented stamp would
/// claim provenance that did not happen, and provenance is what R5.2's upgrade
/// path reads to decide whether a template moved. It lands with that stamp.
fn record_store(
    observed: &ObservedStore,
    intent: &LedgerIntent,
    operation: jails_protocol::identity::OperationId,
    generation: u64,
) -> Result<LedgerV2> {
    observed.validate()?;
    let mut store = observed.ledger.clone().unwrap_or_else(|| LedgerV2 {
        written_by: String::new(),
        generation: 0,
        last_operation: None,
        applied: Vec::new(),
        one_shots: Vec::new(),
        resources: Vec::new(),
        outputs: Vec::new(),
        legacy: Vec::new(),
        pending_conflict: None,
    });
    store.written_by = env!("CARGO_PKG_VERSION").to_string();
    store.generation = generation;
    store.last_operation = Some(operation);

    for entity in &intent.entities_after {
        let applied = jails_protocol::record::AppliedEntity {
            id: entity.id.clone(),
            owners: entity.owners.clone(),
            version: jails_protocol::record::AppliedVersion {
                spec: entity.spec.clone(),
                operation,
            },
        };
        match store.applied.iter_mut().find(|row| row.id == applied.id) {
            // An entity whose owners and spec are unchanged keeps the
            // operation that applied it. Stamping the current one would say a
            // transition happened to it when none did, and the store would
            // then differ from itself on every repeat -- which is exactly how
            // "already set up" stops being reachable.
            Some(row)
                if row.owners == applied.owners && row.version.spec == applied.version.spec => {}
            Some(row) => *row = applied,
            None => store.applied.push(applied),
        }
    }
    store.applied.sort_by(|a, b| a.id.cmp(&b.id));

    for desired in &intent.resources_after {
        match store
            .resources
            .iter_mut()
            .find(|row| row.key == desired.key)
        {
            // Owners union rather than replace: two capabilities wanting one
            // dependency both own it, and a request that stated only its own
            // claim must not erase the other's.
            Some(row) => {
                row.owners.extend(desired.owners.iter().cloned());
                row.value = desired.value.clone();
            }
            None => store
                .resources
                .push(jails_protocol::resource::ResourceRecord {
                    key: desired.key.clone(),
                    owners: desired.owners.clone(),
                    value: desired.value.clone(),
                }),
        }
    }
    store.resources.sort_by(|a, b| a.key.cmp(&b.key));

    for desired in &intent.one_shots_after {
        let receipt = jails_protocol::record::OneShotReceipt {
            id: desired.id.clone(),
            spec: desired.spec.clone(),
            state: desired.state,
            lifecycle: desired.lifecycle.clone(),
            operation,
        };
        match store.one_shots.iter_mut().find(|row| row.id == receipt.id) {
            // Same rule as an entity: a receipt whose content is unchanged
            // keeps the operation that wrote it.
            Some(row)
                if row.spec == receipt.spec
                    && row.state == receipt.state
                    && row.lifecycle == receipt.lifecycle => {}
            Some(row) => *row = receipt,
            None => store.one_shots.push(receipt),
        }
    }
    store.one_shots.sort_by(|a, b| a.id.cmp(&b.id));

    // Sorted by the encoder's own rule rather than by a second one here: the
    // legacy key is private to the envelope, and a copy of the ordering would
    // be a second authority on what canonical means.
    store.legacy = intent.legacy_after.clone();
    Ok(store)
}

fn intern(objects: &mut ObjectBundle, body: Vec<u8>) -> ObjectRef {
    let id = ObjectId::from_bytes(sha256(&body));
    let len = body.len() as u64;
    objects.entry(id).or_insert_with(|| Arc::from(body));
    ObjectRef::new(id, len)
}

/// Step 9: the directories a create needs, and only a create.
///
/// Never for a delete. An empty directory left behind is untidy; a removed
/// one the user had put something in is data loss, and a listing captured
/// earlier cannot tell the two apart at commit time.
fn parents(base: &ProjectSnapshot, operations: &[FileOp]) -> Result<Vec<DirectoryOp>> {
    let mut needed: BTreeSet<ProjectPath> = BTreeSet::new();
    for operation in operations {
        let FileOp::Create { path, .. } = operation else {
            continue;
        };
        let OperationTarget::Project(path) = path else {
            continue;
        };
        let mut components: Vec<&str> = path.as_str().split('/').collect();
        components.pop();
        while !components.is_empty() {
            let candidate = components.join("/");
            // `.jails` is executor-owned machine structure. It is created by
            // the bootstrap, not by a plan, so a `DirectoryOp` for it would
            // be two owners for one directory.
            if candidate == ".jails" || candidate.starts_with(".jails/") {
                break;
            }
            let parent = ProjectPath::parse(&candidate)?;
            // A parent that is already a captured *file* cannot become a
            // directory, and finding that out at commit time would leave the
            // transaction half applied.
            if matches!(base.read(&parent), Ok(Captured::Present(_))) {
                return Err(format!(
                    "`{parent}` is a file, and `{path}` needs it to be a directory"
                ));
            }
            needed.insert(parent);
            components.pop();
        }
    }
    Ok(needed
        .into_iter()
        .map(|path| DirectoryOp::Create { path })
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    base: Arc<ProjectSnapshot>,
    semantics: OperationSemanticsV1,
    kind: PreparedKind,
    operations: Vec<FileOp>,
    directories: Vec<DirectoryOp>,
    objects: ObjectBundle,
    context: PreparationContext,
) -> Result<PreparedBundle> {
    let operation_identity = OperationIdentityV1 {
        snapshot: jails_protocol::snapshot::snapshot_digest(&context.read_set)?,
        operation_context: context.operation_context,
        invocation: None,
        proposed_generation: context.observed_generation + 1,
        semantics,
    };
    let operation_id = operation_identity.operation_id()?;

    let mut change = PreparedChange {
        operation_identity,
        operation_id,
        transaction_id: jails_protocol::identity::TransactionId::from_bytes([0; 32]),
        preparation: context.preparation,
        input_preconditions: context.read_set.inputs().to_vec(),
        operations,
        directories,
        ledger_before: context.observed_store.image,
        ledger_after: FileImage::Absent,
        objects,
        post_commit: Vec::new(),
        kind,
    };
    if let OperationSemanticsV1::Apply(apply) = &change.operation_identity.semantics {
        let store = record_store(
            &context.observed_store,
            &apply.ledger_intent,
            change.operation_id,
            change.operation_identity.proposed_generation,
        )?;
        // A store whose rows are all unchanged is not rewritten. Bumping the
        // generation and stamping a new operation id for a run that did
        // nothing would make every `--pretend`-shaped repeat look like a
        // transition, and `is_no_op` -- which is what the caller reports as
        // "already set up" -- would never be true again.
        if unchanged(&store, &context.observed_store.ledger)
            && change.operations.is_empty()
            && change.directories.is_empty()
        {
            change.ledger_after = change.ledger_before;
        } else {
            let bytes = store.render()?.into_bytes();
            change.ledger_after = FileImage::Present {
                object: intern(&mut change.objects, bytes),
                mode: default_mode(),
            };
        }
    }
    change.transaction_id = change.identity()?.transaction_id()?;
    change.validate()?;
    Ok(PreparedBundle {
        root: base.root().clone(),
        change,
    })
}

fn default_mode() -> FileMode {
    FileMode::new(0o644).expect("0o644 is a permission mode")
}

/// The subject a prepared apply describes, for a report.
pub fn subject_of(change: &PreparedChange) -> Option<&PlannedSubject> {
    match &change.operation_identity.semantics {
        OperationSemanticsV1::Apply(apply) => Some(&apply.subject),
        _ => None,
    }
}

/// The migration a prepared apply carries, if it is a first schema-2 commit.
pub fn migration_of(change: &PreparedChange) -> Option<&LegacyMigrationIdentity> {
    match &change.operation_identity.semantics {
        OperationSemanticsV1::Apply(apply) => apply.migration.as_ref(),
        _ => None,
    }
}

/// The owners a file operation is charged to, for a report.
pub fn contributors_of(operation: &FileOp) -> &BTreeSet<ResourceOwner> {
    operation.contributors()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jails_project::pom::Flavor;
    use jails_protocol::bootstrap::Bootstrap;
    use jails_protocol::entity::{EntityId, IntentId, Recipe};
    use jails_protocol::envelope::LedgerV2;
    use jails_protocol::identity::{Name, Package, TemplateId};
    use jails_protocol::plan::{DesiredChangeSet, LedgerIntent};
    use jails_protocol::provenance::TemplateOrigin;
    use jails_protocol::render::{DesiredFile, TemplateBindings};
    use jails_protocol::resource::{DesiredResource, ResourceKey, ResourceValue};
    use jails_protocol::snapshot::{
        CanonicalRoot, InputPrecondition, MachineRootPresence, ResolvedTemplate, SnapshotFile,
    };
    use jails_spec::build::Build;

    fn path(text: &str) -> ProjectPath {
        ProjectPath::parse(text).unwrap()
    }

    fn owner(name: &str) -> ResourceOwner {
        ResourceOwner::Entity(EntityId::Intent(IntentId::new(
            Recipe::Record,
            Name::parse(name).unwrap(),
            Package::parse("com.example.demo.domain").unwrap(),
        )))
    }

    fn snapshot(files: &[(&str, &str)], absences: &[&str]) -> Arc<ProjectSnapshot> {
        let mut captured = BTreeMap::new();
        for (name, body) in files {
            captured.insert(
                path(name),
                SnapshotFile::capture(body.as_bytes().to_vec(), default_mode()),
            );
        }
        Arc::new(
            ProjectSnapshot::new(
                CanonicalRoot::new("/srv/demo").unwrap(),
                captured,
                absences.iter().map(|name| path(name)).collect(),
                BTreeMap::new(),
            )
            .unwrap(),
        )
    }

    fn projection(base: Arc<ProjectSnapshot>) -> ProjectedProject {
        ProjectedProject::new(
            base,
            Build::Maven,
            Package::parse("com.example.demo").unwrap(),
            25,
            Some(Flavor::PlainMaven),
        )
    }

    fn read_set(base: &ProjectSnapshot) -> ReadSet {
        base.read_set().unwrap()
    }

    fn context(
        base: &ProjectSnapshot,
        templates: TemplateStore,
        generation: u64,
    ) -> PreparationContext {
        PreparationContext {
            read_set: read_set(base),
            templates,
            observed_generation: generation,
            // These tests are about the diff, not about the store, so they
            // plan against a project that has had no transition. The
            // generation they claim is passed separately on purpose: it is
            // what lets `applying_against_a_moved_store_is_refused` state its
            // property without building a whole ledger file.
            observed_store: ObservedStore::default(),
            operation_context: OperationContextFingerprint::default(),
            preparation: PreparationContextFingerprint::default(),
        }
    }

    fn ready() -> LoadedProject {
        Bootstrap::begin(
            CanonicalRoot::new("/srv/demo").unwrap(),
            MachineRootPresence::Present,
        )
        .with_ledger(Some(LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 3,
            last_operation: None,
            applied: Vec::new(),
            legacy: Vec::new(),
            pending_conflict: None,
            one_shots: Vec::new(),
            resources: Vec::new(),
            outputs: Vec::new(),
        }))
        .unwrap()
        .classify()
        .unwrap()
    }

    /// A change that writes one file, and the intent that agrees with it.
    /// A change that writes a shared file without claiming it.
    ///
    /// The distinction matters since R5.3's reconciliation reached the diff: a
    /// path an entity *owns* and finds already occupied is refused, because
    /// jails did not write those bytes. A `pom.xml` is not owned by anybody --
    /// it is edited -- so the properties below are stated about one of those.
    fn edit_one(at: &str, body: &[u8]) -> DesiredChangeSet {
        let mut set = write_one(at, body);
        for change in &mut set.ordered {
            change.resources.clear();
            for file in &mut change.files {
                file.resource = None;
            }
        }
        set.ledger_intent.resources_after.clear();
        set
    }

    fn write_one(at: &str, body: &[u8]) -> DesiredChangeSet {
        let key = ResourceKey::WholeFile(path(at));
        let mut change = DesiredChange::owned_by(owner("Note"));
        change.resources.push(
            DesiredResource::new(
                key.clone(),
                BTreeSet::from([owner("Note")]),
                ResourceValue::WholeFile,
            )
            .unwrap(),
        );
        change.files.push(DesiredFile {
            path: path(at),
            body: DesiredBody::Bytes(body.to_vec().into()),
            mode: None,
            resource: Some(key.clone()),
        });
        DesiredChangeSet {
            ordered: vec![change],
            subject: PlannedSubject::Reconcile(
                jails_protocol::ownership::DesiredState::new(
                    jails_protocol::ownership::ReconcileScope::AppManifest,
                    BTreeMap::new(),
                )
                .unwrap(),
            ),
            ledger_intent: LedgerIntent {
                generation_before: 3,
                entities_after: Vec::new(),
                one_shots_after: Vec::new(),
                resources_after: vec![
                    DesiredResource::new(
                        key,
                        BTreeSet::from([owner("Note")]),
                        ResourceValue::WholeFile,
                    )
                    .unwrap(),
                ],
                legacy_after: Vec::new(),
            },
        }
    }

    fn run(
        base: Arc<ProjectSnapshot>,
        set: DesiredChangeSet,
        generation: u64,
    ) -> Result<PreparedBundle> {
        let context = context(&base, TemplateStore::default(), generation);
        prepare(
            &ready(),
            CommitPlan::Apply(set),
            base.clone(),
            projection(base),
            context,
        )
    }

    #[test]
    fn a_new_file_becomes_a_create_with_its_owner() {
        let base = snapshot(&[], &["src/main/java/com/example/demo/domain/Note.java"]);
        let bundle = run(
            base,
            write_one(
                "src/main/java/com/example/demo/domain/Note.java",
                b"record Note() {}\n",
            ),
            3,
        )
        .unwrap();

        assert_eq!(bundle.change.operations.len(), 1);
        let FileOp::Create { contributors, .. } = &bundle.change.operations[0] else {
            panic!("expected a create, got {:?}", bundle.change.operations[0]);
        };
        assert_eq!(contributors, &BTreeSet::from([owner("Note")]));
        bundle.change.validate().unwrap();
    }

    /// Equal bytes *and* mode emit no operation. A file with the right bytes
    /// and the wrong mode is not the file that was meant.
    ///
    /// It is still a transition, though, and that is not a contradiction: this
    /// request claims a resource the store has never recorded, and a claim
    /// nobody wrote down is a claim `remove` cannot honour. The no-op case is
    /// the one below it -- same bytes *and* nothing new to say.
    #[test]
    fn writing_the_bytes_that_are_already_there_emits_no_file_operation() {
        let base = snapshot(&[("pom.xml", "<project/>")], &[]);
        let bundle = run(base, write_one("pom.xml", b"<project/>"), 3).unwrap();
        assert!(bundle.change.operations.is_empty());
        assert!(
            !bundle.change.is_no_op(),
            "the resource claim is new, so the store moves"
        );
    }

    /// A request that changes no bytes *and* records nothing new writes
    /// nothing at all -- not even a generation bump.
    ///
    /// Rewriting the store for a run that did nothing would make every repeat
    /// look like a transition, and "already set up -- nothing to do" would
    /// never be true again.
    #[test]
    fn a_request_with_nothing_to_say_is_a_no_op() {
        let base = snapshot(&[("pom.xml", "<project/>")], &[]);
        let mut set = write_one("pom.xml", b"<project/>");
        set.ledger_intent.resources_after.clear();
        for change in &mut set.ordered {
            change.resources.clear();
            for file in &mut change.files {
                file.resource = None;
            }
        }
        let bundle = run(base, set, 3).unwrap();
        assert!(bundle.change.is_no_op());
    }

    #[test]
    fn changed_bytes_become_a_replace_that_guards_the_preimage() {
        let base = snapshot(&[("pom.xml", "<project/>")], &[]);
        let bundle = run(base, edit_one("pom.xml", b"<project></project>"), 3).unwrap();
        let FileOp::Replace { before, .. } = &bundle.change.operations[0] else {
            panic!("expected a replace");
        };
        assert_eq!(before.object.len, "<project/>".len() as u64);
    }

    /// Applying against a store that has moved on would write a store that
    /// never existed.
    #[test]
    fn a_plan_computed_against_another_generation_is_refused() {
        let base = snapshot(&[], &["pom.xml"]);
        let error = run(base, write_one("pom.xml", b"<project/>"), 7).unwrap_err();
        assert!(error.contains("computed against generation 3"), "{error}");
    }

    /// The files and the store are computed from one plan; a difference means
    /// they would describe different projects.
    #[test]
    fn changes_that_disagree_with_the_ledger_intent_are_refused() {
        let base = snapshot(&[], &["pom.xml"]);
        let mut set = write_one("pom.xml", b"<project/>");
        set.ledger_intent.resources_after.clear();
        let error = run(base, set, 3).unwrap_err();
        assert!(
            error.contains("no change claims it") || error.contains("does not record it"),
            "{error}"
        );
    }

    #[test]
    fn a_deferred_render_is_materialised_exactly_once() {
        let base = snapshot(&[], &["src/main/java/com/example/demo/App.java"]);
        let mut set = write_one("src/main/java/com/example/demo/App.java", b"");
        let mut bindings = TemplateBindings::new();
        bindings
            .bind(
                TemplateKey::parse("name").unwrap(),
                TemplateValue::Name(Name::parse("App").unwrap()),
            )
            .unwrap();
        set.ordered[0].files[0].body = DesiredBody::Render {
            template: TemplateId::parse("app_java").unwrap(),
            bindings,
        };
        let templates = TemplateStore::new(vec![ResolvedTemplate::capture(
            TemplateId::parse("app_java").unwrap(),
            TemplateOrigin::BuiltIn {
                name: TemplateId::parse("app_java").unwrap(),
            },
            "class {{name}} {}\n",
            BTreeSet::from([TemplateKey::parse("name").unwrap()]),
        )])
        .unwrap();

        let bundle = prepare(
            &ready(),
            CommitPlan::Apply(set),
            base.clone(),
            projection(base.clone()),
            context(&base, templates, 3),
        )
        .unwrap();

        let FileOp::Create { after, .. } = &bundle.change.operations[0] else {
            panic!("expected a create");
        };
        assert_eq!(bundle.change.objects[&after.id].as_ref(), b"class App {}\n");
    }

    /// An unresolved template would render from bytes nothing recorded — the
    /// same discipline as an undeclared read.
    #[test]
    fn rendering_an_unresolved_template_is_refused() {
        let base = snapshot(&[], &["src/main/java/com/example/demo/App.java"]);
        let mut set = write_one("src/main/java/com/example/demo/App.java", b"");
        set.ordered[0].files[0].body = DesiredBody::Render {
            template: TemplateId::parse("app_java").unwrap(),
            bindings: TemplateBindings::new(),
        };
        let error = run(base, set, 3).unwrap_err();
        assert!(error.contains("was not resolved"), "{error}");
    }

    #[test]
    fn parents_are_derived_for_creates_and_stop_at_the_machine_root() {
        let base = snapshot(&[], &["src/main/java/com/example/demo/domain/Note.java"]);
        let bundle = run(
            base,
            write_one("src/main/java/com/example/demo/domain/Note.java", b"x"),
            3,
        )
        .unwrap();
        let made: Vec<String> = bundle
            .change
            .directories
            .iter()
            .map(|d| d.path().to_string())
            .collect();
        assert_eq!(
            made,
            vec![
                "src",
                "src/main",
                "src/main/java",
                "src/main/java/com",
                "src/main/java/com/example",
                "src/main/java/com/example/demo",
                "src/main/java/com/example/demo/domain",
            ]
        );
    }

    /// Finding this out at commit time would leave the transaction half
    /// applied.
    #[test]
    fn a_parent_that_is_a_file_is_refused_before_anything_happens() {
        let base = snapshot(&[("src", "not a directory")], &["src/App.java"]);
        let error = run(base, write_one("src/App.java", b"x"), 3).unwrap_err();
        assert!(error.contains("needs it to be a directory"), "{error}");
    }

    /// Never for a delete: a removed directory the user had put something in
    /// is data loss, and a captured listing cannot tell that apart at commit.
    #[test]
    fn a_delete_derives_no_directory_at_all() {
        let base = snapshot(&[("src/main/java/App.java", "x")], &[]);
        let mut set = write_one("src/main/java/App.java", b"x");
        set.ordered[0].files.clear();
        set.ordered[0]
            .absences
            .push(jails_protocol::render::ManagedPath {
                path: path("src/main/java/App.java"),
                resource: ResourceKey::WholeFile(path("src/main/java/App.java")),
                force: false,
            });
        let bundle = run(base, set, 3).unwrap();
        assert!(matches!(bundle.change.operations[0], FileOp::Delete { .. }));
        assert!(bundle.change.directories.is_empty());
    }

    /// The read set enters the prepared value whole: the commit-time stale
    /// check has nothing to compare otherwise.
    #[test]
    fn every_declared_input_becomes_a_precondition() {
        let base = snapshot(&[("pom.xml", "<project/>")], &["compose.yaml"]);
        let bundle = run(base.clone(), edit_one("pom.xml", b"<project></project>"), 3).unwrap();
        let paths: BTreeSet<String> = bundle
            .change
            .input_preconditions
            .iter()
            .map(|input| match input {
                InputPrecondition::Absent { path } | InputPrecondition::File { path, .. } => {
                    path.to_string()
                }
                other => format!("{other:?}"),
            })
            .collect();
        assert!(paths.contains("pom.xml"), "{paths:?}");
        assert!(paths.contains("compose.yaml"), "{paths:?}");
    }

    /// Finalisation has no file or directory operation: the resolved bytes
    /// are already on disk, and rewriting them would discard the resolution.
    #[test]
    fn finalisation_touches_no_file() {
        let base = snapshot(&[("pom.xml", "<project/>")], &[]);
        let origin = jails_protocol::transition::ConflictOrigin {
            operation: jails_protocol::identity::OperationId::from_bytes(sha256(b"op")),
            transaction: jails_protocol::identity::TransactionId::from_bytes(sha256(b"tx")),
            generation: 3,
            receipt: jails_protocol::transition::ReceiptGuard {
                transaction: jails_protocol::identity::TransactionId::from_bytes(sha256(b"tx")),
                generation: 3,
                record_checksum: ObjectId::from_bytes(sha256(b"receipt")),
            },
            pending: jails_protocol::conflict::PendingIdentity::from_object(ObjectId::from_bytes(
                sha256(b"pending"),
            )),
        };
        let pending = Bootstrap::begin(
            CanonicalRoot::new("/srv/demo").unwrap(),
            MachineRootPresence::Present,
        )
        .with_ledger(Some(LedgerV2 {
            written_by: "0.1.0".to_string(),
            generation: 3,
            last_operation: None,
            applied: Vec::new(),
            legacy: Vec::new(),
            pending_conflict: Some(jails_protocol::envelope::PendingMarker {
                operation: jails_protocol::identity::OperationId::from_bytes(sha256(b"op")),
                generation: 3,
                request_syntax: jails_protocol::request::CanonicalRequestSyntaxV1::default()
                    .fingerprint()
                    .unwrap(),
                resume_display: "jails app apply".to_string(),
            }),
            one_shots: Vec::new(),
            resources: Vec::new(),
            outputs: Vec::new(),
        }))
        .unwrap()
        .classify()
        .unwrap();

        let bundle = prepare(
            &pending,
            CommitPlan::Finalise(FinalisationPlan {
                origin,
                resolutions: Vec::new(),
                effect_intents: Vec::new(),
            }),
            base.clone(),
            projection(base.clone()),
            context(&base, TemplateStore::default(), 3),
        )
        .unwrap();
        assert!(bundle.change.operations.is_empty());
        assert!(bundle.change.directories.is_empty());
        assert!(bundle.change.post_commit.is_empty());
    }
}
