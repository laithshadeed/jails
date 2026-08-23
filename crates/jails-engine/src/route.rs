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
use jails_prepare::pipeline::{self, PreparationContext};
use jails_project::capture::{self, ReadDeclaration};
use jails_project::model::{Change, Project};
use jails_protocol::bootstrap::Bootstrap;
use jails_protocol::change::DesiredChange;
use jails_protocol::edit::SemanticEdit;
use jails_protocol::entity::{
    CapabilityId, CapabilityInstance, CapabilitySpec, EntityId, EntitySpec, OwnerId,
};
use jails_protocol::identity::ProjectPath;
use jails_protocol::ownership::{DesiredEntity, DesiredState, ReconcileScope};
use jails_protocol::plan::{DesiredAppliedEntity, DesiredChangeSet, LedgerIntent, PlannedSubject};
use jails_protocol::resource::{DesiredResource, ResourceKey, ResourceOwner, ResourceValue};
use jails_protocol::snapshot::{MachineRootPresence, TemplateStore};
use jails_protocol::transition::CommitPlan;
use jails_spec::spec::kind::Capability;
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
    let set = change_set(ReconcileScope::DirectConfig, entity, desired)?;
    commit(project, set, &declaration(project, &change)?, "jails add")
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

/// Everything one request wants, as the complete state of the scope it speaks
/// for.
///
/// A scope may only ever add or remove *its own* claim, so the ledger intent
/// carries this entity and the resources this change charges to it, and says
/// nothing about anybody else's.
fn change_set(
    scope: ReconcileScope,
    entity: DesiredEntity,
    change: DesiredChange,
) -> Result<DesiredChangeSet> {
    let applied = DesiredAppliedEntity {
        id: entity.id.clone(),
        owners: entity.owners.clone(),
        spec: entity.spec.clone(),
    };
    let state = DesiredState::new(scope, BTreeMap::from([(entity.id.clone(), entity)]))?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            // A first transition against a store that does not exist yet is
            // generation zero, and the executor rechecks it under the lock:
            // a plan computed against a store that has since moved on is
            // refused rather than applied to a state it never saw.
            generation_before: 0,
            entities_after: vec![applied],
            one_shots_after: Vec::new(),
            resources_after: change.resources.clone(),
            legacy_after: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::Reconcile(state),
    };
    set.validate()?;
    Ok(set)
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
    set: DesiredChangeSet,
    declaration: &ReadDeclaration,
    description: &str,
) -> Result<CommitResult> {
    let (snapshot, projection) = capture::projected(project, declaration)?;
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
        observed_generation: 0,
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
