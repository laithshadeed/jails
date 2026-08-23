//! The V2 route, assembled end to end and not yet reachable from dispatch.
//!
//! plan.md §R6.1 is explicit that migration is "incremental in code and tests
//! but **atomic at the production dispatch point**": once one command writes
//! schema 2, an unswitched schema-1 writer cannot safely read or update it, so
//! there is exactly one commit where every command changes at once. Step 1
//! therefore says to land the executor "dark", and steps 2 to 6 to build each
//! command's route while default dispatch stays on V1.
//!
//! This module is where those routes are assembled. Nothing in `main.rs` calls
//! it; the tests do. That is the point — the whole path can be exercised,
//! measured against V1 and crash-tested long before anything depends on it.
//!
//! ## What one route is
//!
//! Seven steps, and each one is a value the next takes:
//!
//! 1. resolve the project, and let the recipe plan what it intends;
//! 2. state that plan as desired resources owned by somebody;
//! 3. capture the project once, and open a projection over the capture;
//! 4. declare the complete desired state for the scope this request speaks for;
//! 5. prepare — render, diff, and turn all of it into exact operations;
//! 6. take the lock;
//! 7. commit, journal-first, ledger-last.
//!
//! The interesting property is that steps 1 to 5 touch nothing. A failure
//! anywhere in them leaves a project that has not been opened for writing.

use std::collections::{BTreeMap, BTreeSet};

use jails_commit::execute::{self, LockedProject, ProjectHandle};
use jails_commit::outcome::{CommitError, CommitResult};
use jails_prepare::desire;
use jails_prepare::pipeline::{self, ObservedStore, PreparationContext};
use jails_project::capture::{self, ReadDeclaration};
use jails_project::model::{Change, Project};
use jails_protocol::bootstrap::Bootstrap;
use jails_protocol::change::DesiredChange;
use jails_protocol::declaration::{FieldSpec, IndexSpec, IntentSpec};
use jails_protocol::edit::SemanticEdit;
use jails_protocol::entity::{
    CapabilityId, CapabilityInstance, CapabilitySpec, EntityId, EntitySpec, IntentId, OwnerId,
};
use jails_protocol::identity::{JavaType, Name, Package, ProjectPath};
use jails_protocol::ownership::{DesiredEntity, DesiredState, ObservedEntity, ReconcileScope};
use jails_protocol::plan::{DesiredAppliedEntity, DesiredChangeSet, LedgerIntent, PlannedSubject};
use jails_protocol::render::ManagedPath;
use jails_protocol::resource::{DesiredResource, ResourceKey, ResourceOwner, ResourceValue};
use jails_protocol::snapshot::{MachineRootPresence, TemplateStore};
use jails_protocol::transition::CommitPlan;
use jails_spec::spec::kind::{ArtifactKind, Capability};
use jails_support::Result;

/// Install one capability through the transaction protocol.
///
/// The direct counterpart of `add::add_in`, and deliberately the same subject:
/// `ReconcileScope::DirectConfig` speaks for the capability list in
/// `jails.toml`, which is what `sync` later reconciles against.
pub fn install(project: &Project, capability: Capability) -> Result<CommitResult> {
    let id = CapabilityId {
        kind: capability,
        instance: CapabilityInstance::Singleton,
    };
    let owner = ResourceOwner::Entity(EntityId::Capability(id.clone()));
    let change = with_test_support(project, jails_generate::add::plan_for(capability, project)?);
    let mut desired = desire::contribution(&owner, &change, project)?;
    record_capability(&mut desired, &owner, &id)?;
    let entity = DesiredEntity {
        id: EntityId::Capability(id),
        spec: EntitySpec::Capability(CapabilitySpec { placement: None }),
        owners: BTreeSet::from([OwnerId::DirectConfig]),
    };
    let request = Request {
        scope: ReconcileScope::DirectConfig,
        declared: declared_capabilities(&observed(project)?, Some(entity))?,
        change: desired,
    };
    commit(
        project,
        request,
        &declaration(project, &change)?,
        "jails add",
    )
}

/// Take one capability back out.
///
/// The exact inverse of [`install`], and it is not a mirrored undo: the
/// request simply stops declaring the capability, and reconciliation works out
/// what that means. A dependency two capabilities wanted stays, because the
/// other one still claims it. A file only this capability owned becomes an
/// absence. The line in `jails.toml` goes, because it was a resource this
/// entity owned like any other.
pub fn remove(project: &Project, capability: Capability) -> Result<CommitResult> {
    let id = CapabilityId {
        kind: capability,
        instance: CapabilityInstance::Singleton,
    };
    let entity = EntityId::Capability(id);
    let store = observed(project)?;
    if !store
        .ledger
        .as_ref()
        .is_some_and(|ledger| ledger.applied.iter().any(|row| row.id == entity))
    {
        return Err(format!(
            "`{}` is not recorded as installed in this project.\n       fix: `jails doctor` says \
             what is installed. Removing something the store never recorded would mean guessing \
             which lines to take out of files jails does not own.",
            capability.label()
        ));
    }
    let mut declared = declared_capabilities(&store, None)?;
    declared.remove(&entity);
    let owner = ResourceOwner::Entity(entity.clone());
    let request = Request {
        scope: ReconcileScope::DirectConfig,
        declared,
        // Nothing is *written* by a removal. Everything it does falls out of
        // the claims it stops making, which is what makes `remove` the exact
        // inverse of `add` rather than a second hand-written description of
        // what `add` did.
        change: DesiredChange::owned_by(owner.clone()),
    };
    commit(project, request, &retiring(&store, &owner)?, "jails remove")
}

/// Generate one persistent artifact through the transaction protocol.
///
/// The direct counterpart of `generate_in_project`, and the subject is one
/// entity rather than the capability list: `ReconcileScope::DirectEntity` is
/// "exactly one direct `generate`/`destroy` request", so this route may add or
/// remove its own claim and says nothing about anybody else's.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    project: &Project,
    kind: ArtifactKind,
    name: &str,
    fields: &[String],
    package: Option<&str>,
    indexes: &[String],
    on: Option<&str>,
    yields: Option<&str>,
) -> Result<CommitResult> {
    let change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project, kind, name, fields, package, indexes, on, yields,
        )?,
    );
    let id = intent(project, kind, name, package, fields, indexes, on, yields)?;
    let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
    let desired = desire::contribution(&owner, &change, project)?;
    let entity = DesiredEntity {
        id: EntityId::Intent(id.clone()),
        spec: EntitySpec::Intent(spec(project, fields, indexes, on, yields)?),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let request = Request {
        scope: ReconcileScope::DirectEntity(EntityId::Intent(id)),
        declared: BTreeMap::from([(entity.id.clone(), entity)]),
        change: desired,
    };
    commit(
        project,
        request,
        &declaration(project, &change)?,
        "jails generate",
    )
}

/// `(recipe, name, resolved package)` — the identity everything about this
/// artifact is filed under.
///
/// The package is resolved rather than optional: two rows for one artifact,
/// one saying "wherever the convention puts it" and one naming the package it
/// went to, are two authorities for one identity.
#[allow(clippy::too_many_arguments)]
fn intent(
    project: &Project,
    kind: ArtifactKind,
    name: &str,
    package: Option<&str>,
    _fields: &[String],
    _indexes: &[String],
    _on: Option<&str>,
    _yields: Option<&str>,
) -> Result<IntentId> {
    Ok(IntentId {
        recipe: kind,
        name: Name::parse(&jails_generate::generate::strip_redundant_suffix(
            kind,
            &jails_spec::spec::field::capitalize(name),
        ))?,
        package: Package::parse(&project.package_named("", package))?,
    })
}

/// What the artifact was asked for, as the content of its identity.
fn spec(
    project: &Project,
    fields: &[String],
    indexes: &[String],
    on: Option<&str>,
    yields: Option<&str>,
) -> Result<IntentSpec> {
    let base = Package::parse(project.base())?;
    let mut parsed = Vec::new();
    for token in fields {
        parsed.push(FieldSpec::parse(token, &base)?);
    }
    let mut declared = Vec::new();
    for index in indexes {
        declared.push(IndexSpec::parse(index, &parsed)?);
    }
    Ok(IntentSpec {
        fields: parsed,
        indexes: declared,
        // `--timestamps` is expanded into fields before a recipe ever sees it,
        // so by the time there is a spec the two extra components are ordinary
        // ones. Recording it again would make one request two facts.
        timestamps: false,
        on: on.map(JavaType::parse).transpose()?,
        yields: yields.map(JavaType::parse).transpose()?,
    })
}

/// The two things the write path adds to any change that writes tests.
///
/// A capability or a generator that emits a test emits it against AssertJ, and
/// one that emits an `*IT` needs Failsafe -- which is *not* in the Spring Boot
/// parent's default build, so without it `mvn verify` completes, reports
/// success and runs none of them. jails generated integration tests for months
/// that never ran once.
///
/// The direct write path applies both from `write_new_file`/`add_in` rather
/// than per recipe, for the same reason the Java shape rules live below every
/// producer: a rule twenty recipes have to remember is a rule that decays. So
/// every route applies them here, once, to whatever it is about to desire.
fn with_test_support(project: &Project, mut change: Change) -> Change {
    let writes = |suffix: &str| {
        change
            .files
            .iter()
            .any(|file| file.path.to_string_lossy().contains(suffix))
    };
    if writes("src/test/java")
        && !jails_project::pom::has_dependency(project.pom(), "org.assertj", "assertj-core")
        && !project.pom().contains("spring-boot-starter-test")
        && !project.pom().contains("spring-boot-starter-webmvc-test")
    {
        change
            .deps
            .push(jails_project::pom::assertj(project.flavor()));
    }
    if writes("IT.java") {
        change.plugins.push((
            jails_generate::spring::FAILSAFE_ARTIFACT,
            jails_generate::spring::failsafe_plugin(project.flavor()).to_string(),
        ));
    }
    change
}

/// Record the capability in the manifest `sync` acts on.
///
/// CLAUDE.md states the rule and the reason: a manifest somebody has to
/// remember to update is a manifest that is wrong, and a wrong one is worse
/// than none because `sync` acts on it. It is a resource rather than a side
/// effect, so removing the capability takes the line out by the same
/// mechanism that put it in.
fn record_capability(
    change: &mut DesiredChange,
    owner: &ResourceOwner,
    id: &CapabilityId,
) -> Result<()> {
    let key = ResourceKey::HumanConfigCapability(id.clone());
    let spec = CapabilitySpec { placement: None };
    change.resources.push(DesiredResource::new(
        key.clone(),
        BTreeSet::from([owner.clone()]),
        ResourceValue::HumanConfigCapability(spec.clone()),
    )?);
    change
        .edits
        .push(SemanticEdit::HumanConfigCapability { key, spec });
    Ok(())
}

/// What a removal is allowed to read: the format owners, plus every file this
/// owner is about to give up.
///
/// A file is deleted against a *guarded preimage*, and the guard is only
/// meaningful if the file was captured. Declaring the ones that are leaving --
/// rather than every file jails has ever written -- keeps the preconditions to
/// what this request actually depends on, so an unrelated generated file
/// changing does not make the removal refuse.
fn retiring(store: &ObservedStore, owner: &ResourceOwner) -> Result<ReadDeclaration> {
    let mut declaration = capture::capability_reads()?;
    for row in store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
    {
        if let ResourceKey::WholeFile(path) = &row.key
            && row.owners.iter().all(|held| held == owner)
        {
            declaration = declaration.file(path.clone());
        }
    }
    Ok(declaration)
}

/// The store as it is, read once per command.
fn observed(project: &Project) -> Result<ObservedStore> {
    jails_commit::store::Store::at(project.root()).observe()
}

/// Every capability `jails.toml`'s scope currently declares, with `changed`
/// applied to it.
///
/// `DirectConfig` speaks for the *whole* capability list, so a request that
/// declared only the capability it is installing would be saying every other
/// capability is no longer wanted -- and the reconciler would dutifully
/// retire them. Passing `None` for `changed` is how a removal says one is
/// gone.
fn declared_capabilities(
    observed: &ObservedStore,
    changed: Option<DesiredEntity>,
) -> Result<BTreeMap<EntityId, DesiredEntity>> {
    let mut declared = BTreeMap::new();
    if let Some(store) = &observed.ledger {
        for row in &store.applied {
            if !matches!(row.id, EntityId::Capability(_))
                || !row.owners.contains(&OwnerId::DirectConfig)
            {
                continue;
            }
            declared.insert(
                row.id.clone(),
                DesiredEntity {
                    id: row.id.clone(),
                    spec: row.version.spec.clone(),
                    owners: BTreeSet::from([OwnerId::DirectConfig]),
                },
            );
        }
    }
    if let Some(entity) = changed {
        declared.insert(entity.id.clone(), entity);
    }
    Ok(declared)
}

/// One request, before it is measured against the store.
///
/// Deliberately not a `DesiredChangeSet` yet. That value states what the store
/// looks like afterwards, and afterwards is a function of what is there now --
/// which exactly one place may read (see [`commit`]). A field filled with a
/// placeholder here and corrected there is two authorities on one number, and
/// the executor refuses when they disagree.
struct Request {
    scope: ReconcileScope,
    /// What this scope declares. Empty is a real declaration: it says this
    /// scope wants nothing, which is how a removal is expressed.
    declared: BTreeMap<EntityId, DesiredEntity>,
    change: DesiredChange,
}

impl Request {
    /// Measure this request against the store, and say what the store becomes.
    ///
    /// The reconciliation is [`jails_protocol::ownership::reconcile`]'s, not a
    /// second copy of it: a scope may only add or remove *its own* claim, an
    /// owner outside the scope is carried forward untouched, and an entity
    /// whose last owner leaves is removed. What is left here is projecting
    /// that answer onto the resource rows -- a resource loses the owners whose
    /// entities went, and a resource nobody claims any more is retired.
    fn against(self, observed: &ObservedStore) -> Result<DesiredChangeSet> {
        let recorded = observed.ledger.as_ref();
        let applied: BTreeMap<EntityId, ObservedEntity> = recorded
            .map(|store| {
                store
                    .applied
                    .iter()
                    .map(|row| {
                        (
                            row.id.clone(),
                            ObservedEntity {
                                spec: row.version.spec.clone(),
                                owners: row.owners.clone(),
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let reconciled =
            jails_protocol::ownership::reconcile(&self.scope, &self.declared, &applied, &[])?;

        let entities_after = reconciled
            .entities
            .iter()
            .map(|(id, entity)| DesiredAppliedEntity {
                id: id.clone(),
                owners: entity.owners.clone(),
                spec: entity.spec.clone(),
            })
            .collect();
        let gone: BTreeSet<ResourceOwner> = reconciled
            .removed
            .iter()
            .map(|id| ResourceOwner::Entity(id.clone()))
            .collect();

        // Which resources this removal leaves unowned. Computed here only to
        // decide what has to come *out of the files*; the store derives the
        // same answer from `entities_removed`, so the two cannot disagree
        // about which rows survive.
        let mut change = self.change;
        for row in recorded
            .map(|store| store.resources.as_slice())
            .unwrap_or(&[])
        {
            if row.owners.iter().any(|owner| !gone.contains(owner)) {
                continue;
            }
            match &row.key {
                // A whole file leaves as an absence rather than an edit: the
                // executor guards the preimage it deletes, which an edit
                // cannot do.
                ResourceKey::WholeFile(path) => change.absences.push(ManagedPath {
                    path: path.clone(),
                    resource: row.key.clone(),
                    force: false,
                }),
                _ => change.edits.push(SemanticEdit::Retire {
                    key: row.key.clone(),
                }),
            }
        }

        // Exactly the claims this request makes, and no more. The intent
        // speaks for one scope, and `require_intent_matches` holds it to
        // saying the same thing the changes do.
        let resources_after = change.resources.clone();

        let set = DesiredChangeSet {
            ledger_intent: LedgerIntent {
                generation_before: observed.generation(),
                entities_after,
                one_shots_after: Vec::new(),
                resources_after,
                entities_removed: reconciled.removed,
                legacy_after: Vec::new(),
            },
            ordered: vec![change],
            subject: PlannedSubject::Reconcile(DesiredState::new(self.scope, self.declared)?),
        };
        set.validate()?;
        Ok(set)
    }
}

/// What this request is allowed to read: the format owners, plus every file it
/// intends to write.
///
/// A file it writes has to be declared too, because writing one is a decision
/// about what was there — and "there was nothing there" is exactly the kind of
/// fact the executor rechecks under the lock.
fn declaration(project: &Project, change: &Change) -> Result<ReadDeclaration> {
    let mut declaration = capture::capability_reads()?;
    for artifact in &change.files {
        declaration = declaration.file(relative(project, &artifact.path)?);
    }
    Ok(declaration)
}

/// A planned artifact's path, as the project-relative name a resource has.
///
/// A recipe plans in absolute paths because it writes files; a resource is
/// named by where it sits in the project, so that the same record means the
/// same thing on another machine.
fn relative(project: &Project, path: &std::path::Path) -> Result<ProjectPath> {
    let relative = path.strip_prefix(project.root()).map_err(|_| {
        format!(
            "{} is outside {}, so this request cannot claim it",
            path.display(),
            project.root().display()
        )
    })?;
    let text = relative
        .to_str()
        .ok_or_else(|| format!("{} is not valid UTF-8", relative.display()))?;
    ProjectPath::parse(text)
}

/// Steps 3 and 5 to 7: capture, prepare, lock, commit.
fn commit(
    project: &Project,
    request: Request,
    declaration: &ReadDeclaration,
    description: &str,
) -> Result<CommitResult> {
    let (snapshot, mut projection) = capture::projected(project, declaration)?;
    // Read once, and let the same value decide the generation the plan claims
    // and the image the commit guards under the lock. Reading them apart is
    // how a plan comes to be written against a store that moved in between.
    let observed = observed(project)?;
    if let Some(store) = &observed.ledger {
        projection.record(&store.resources);
    }
    let set = request.against(&observed)?;
    let root = capture::canonical_root(project.root())?;
    let machine = if project.root().join(".jails").is_dir() {
        MachineRootPresence::Present
    } else {
        MachineRootPresence::Absent
    };
    let loaded = Bootstrap::begin(root, machine)
        .with_ledger(None)?
        .classify()?;
    let context = PreparationContext {
        read_set: snapshot.read_set()?,
        // Nothing is rendered from a template on this route yet: a recipe
        // hands over bytes it already produced. An empty store is therefore
        // the honest value, and `TemplateStore::resolve` refuses anything
        // that tries to render from bytes nothing recorded.
        templates: TemplateStore::new(Vec::new())?,
        observed_generation: observed.generation(),
        observed_store: observed,
        operation_context: Default::default(),
        preparation: Default::default(),
    };
    let bundle = pipeline::prepare(
        &loaded,
        CommitPlan::Apply(set),
        snapshot,
        projection,
        context,
    )?;

    let handle = ProjectHandle::at(project.root())?;
    let locked = LockedProject::acquire(handle, description).map_err(describe)?;
    execute::commit(&locked, &bundle).map_err(describe)
}

/// A commit failure as the one line a person reads.
///
/// Every one of these is a refusal before anything was activated -- that is
/// what `CommitError` *means*, and it is why this is a plain message rather
/// than a recovery instruction.
fn describe(error: CommitError) -> String {
    format!("{error:?}")
}
