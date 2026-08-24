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

use clap::ValueEnum;
use jails_commit::execute::{self, LockedProject, ProjectHandle};
use jails_commit::outcome::{CommitError, CommitResult};
use jails_prepare::desire;
use jails_prepare::pipeline::{self, ObservedStore, PreparationContext};
use jails_project::capture::{self, ReadDeclaration};
use jails_project::model::{Change, Project};
use jails_protocol::bootstrap::Bootstrap;
use jails_protocol::change::DesiredChange;
use jails_protocol::context::RenderedSubjectContext;
use jails_protocol::declaration::{FieldSpec, IntentArguments, IntentSpec};
use jails_protocol::edit::SemanticEdit;
use jails_protocol::entity::{
    CapabilityId, CapabilityInstance, CapabilitySpec, EntityId, EntitySpec, IntentId, OneShotId,
    OneShotSpec, OwnerId, SourceInputId,
};
use jails_protocol::identity::{JavaType, Name, ObjectId, Package, ProjectPath};
use jails_protocol::ownership::{DesiredEntity, DesiredState, ObservedEntity, ReconcileScope};
use jails_protocol::plan::{
    DesiredAppliedEntity, DesiredChangeSet, DesiredOneShotReceipt, LedgerIntent, PlannedSubject,
};
use jails_protocol::provenance::{OneShotKind, RendererId};
use jails_protocol::render::{DesiredBody, DesiredFile, ManagedPath};
use jails_protocol::resource::{
    DesiredResource, OneShotLifecycle, OneShotState, ResourceKey, ResourceOwner, ResourceValue,
};
use jails_protocol::snapshot::{MachineRootPresence, TemplateStore};
use jails_protocol::transition::CommitPlan;
use jails_spec::spec::kind::{ArtifactKind, Capability};
use jails_support::Result;

mod artifact;
mod capability;
mod field;
mod oneshot;
mod provenance;

pub use artifact::{destroy, generate};
pub use capability::{install, remove, sync};
pub use field::field;
pub use oneshot::{cases, migration};

/// A kind as the word somebody types, taken from the same `ValueEnum` clap
/// parses -- so a refusal naming `jails g <kind>` names a command that exists.
fn label(kind: ArtifactKind) -> String {
    kind.to_possible_value()
        .expect("every ArtifactKind has a clap value")
        .get_name()
        .to_string()
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
    kind: ArtifactKind,
    arguments: &[String],
    indexes: &[String],
    on: Option<&str>,
    yields: Option<&str>,
) -> Result<IntentSpec> {
    let base = Package::parse(project.base())?;
    // Parsed once to translate the index spelling, then again inside
    // `IntentSpec::parse`, which stays the one authority on what a valid
    // declaration is. Two parses of a handful of tokens is cheaper than two
    // places that decide whether a declaration is well formed.
    let parsed = IntentArguments::parse(kind, arguments, &base)?;
    let translated: Vec<String> = indexes
        .iter()
        .map(|index| as_field_names(index, parsed.fields()))
        .collect();
    let mut spec = IntentSpec::parse(
        kind,
        arguments,
        &translated,
        // `--timestamps` is expanded into fields before a recipe ever sees it,
        // so by the time there is a spec the two extra components are ordinary
        // ones. Recording it again would make one request two facts.
        false,
        &base,
    )?;
    spec.on = on.map(JavaType::parse).transpose()?;
    spec.yields = yields.map(JavaType::parse).transpose()?;
    Ok(spec)
}

/// An `--index` token as the RFC's canonical spelling.
///
/// `IndexSpec` names *fields*, which plan.md §R1.1 fixes deliberately -- the
/// column name is derived, and a spec that stored the derived form would be a
/// second authority on it. But the shipped CLI spelling is the column:
/// `--index "created_at desc"` is what `README.md` documents and what every
/// scenario types, because that is the name the reader sees in the DDL.
///
/// So the column spelling is translated here, at the boundary, rather than
/// either spelling being taught to the protocol or the CLI being changed
/// under people. A token that already names a field passes through untouched,
/// and one that names neither is left exactly as typed so `IndexSpec::parse`
/// produces the refusal that lists the declared fields.
fn as_field_names(token: &str, fields: &[FieldSpec]) -> String {
    token
        .split(',')
        .map(|part| {
            let mut words = part.split_whitespace();
            let Some(first) = words.next() else {
                return String::new();
            };
            let named = fields.iter().find(|field| {
                field.name.as_str() != first
                    && jails_generate::sql::snake_case(field.name.as_str()) == first
            });
            let rest: Vec<&str> = words.collect();
            let head = named.map_or(first, |field| field.name.as_str());
            if rest.is_empty() {
                head.to_string()
            } else {
                format!("{head} {}", rest.join(" "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
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
    // Boot 4 moved `@WebMvcTest` and `@AutoConfigureMockMvc` into a module
    // `spring-boot-starter-test` does not bring in, so a generated test that
    // uses either compiles only when this is declared. Applied here for the
    // same reason the two above are: a rule every recipe has to remember is
    // a rule that decays.
    if jails_generate::generate::writes_a_webmvc_test(&change.files)
        && !jails_project::pom::has_dependency(
            project.pom(),
            "org.springframework.boot",
            "spring-boot-starter-webmvc-test",
        )
    {
        change.deps.push(jails_project::pom::WEBMVC_TEST_STARTER);
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
        if !row.owners.iter().all(|held| held == owner) {
            continue;
        }
        match &row.key {
            ResourceKey::WholeFile(path) => declaration = declaration.file(path.clone()),
            // A surgical edit is undone in the file that holds it, which is a
            // file this owner does not own -- so it has to be declared
            // separately from the ones it does. `add db` splices `@Import`
            // into a test the reader wrote; the retirement reads that test
            // back.
            ResourceKey::SpringTestImport { path, .. } => {
                declaration = declaration.file(path.clone())
            }
            _ => {}
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
    /// One per entity that has something to install. A `sync` that brings two
    /// capabilities in has two, and they commit together or not at all.
    changes: Vec<DesiredChange>,
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

        // Which resources this transition leaves unowned. Computed here only
        // to decide what has to come *out of the files*; the store derives the
        // same answer from `entities_removed`, so the two cannot disagree
        // about which rows survive.
        let mut changes = self.changes;
        let mut retirement: BTreeMap<ResourceOwner, DesiredChange> = BTreeMap::new();
        for row in recorded
            .map(|store| store.resources.as_slice())
            .unwrap_or(&[])
        {
            if row.owners.iter().any(|owner| !gone.contains(owner)) {
                continue;
            }
            // Charged to one of the owners that is leaving, because that is
            // what a change *is* here: work an owner is responsible for. A
            // maintenance attribution would be a lie about who asked, and the
            // change set refuses it under this subject for exactly that
            // reason. The lowest owner is picked so two runs of one removal
            // produce the same transaction.
            let Some(owner) = row.owners.iter().next().cloned() else {
                continue;
            };
            let change = retirement
                .entry(owner.clone())
                .or_insert_with(|| DesiredChange::owned_by(owner));
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
        changes.extend(retirement.into_values());

        // Exactly the claims these changes make, merged the way the projection
        // merges them: one row per key, owners unioned. `require_intent_
        // matches` holds the intent to saying the same thing the changes do.
        let mut merged: BTreeMap<ResourceKey, DesiredResource> = BTreeMap::new();
        for change in &changes {
            for desired in &change.resources {
                match merged.get_mut(&desired.key) {
                    Some(row) => row.owners.extend(desired.owners.iter().cloned()),
                    None => {
                        merged.insert(desired.key.clone(), desired.clone());
                    }
                }
            }
        }
        let resources_after: Vec<DesiredResource> = merged.into_values().collect();

        let set = DesiredChangeSet {
            ledger_intent: LedgerIntent {
                generation_before: observed.generation(),
                entities_after,
                one_shots_after: Vec::new(),
                resources_after,
                entities_removed: reconciled.removed,
                legacy_after: Vec::new(),
            },
            ordered: changes,
            subject: PlannedSubject::Reconcile(DesiredState::new(self.scope, self.declared)?),
        };
        set.validate()?;
        Ok(set)
    }
}

/// What this request is allowed to read: the format owners, plus every file it
/// intends to write, plus every file it intends to edit surgically.
///
/// A file it writes has to be declared too, because writing one is a decision
/// about what was there — and "there was nothing there" is exactly the kind of
/// fact the executor rechecks under the lock.
///
/// The `desired` half is what makes a surgical edit safe. `add db` splices
/// `@Import` into every `@SpringBootTest` it finds, and *which tests exist* is
/// read while planning. Declaring each one turns that read into a
/// precondition, so a test added between the plan and the commit makes this
/// refuse rather than silently miss it.
fn declaration(
    project: &Project,
    change: &Change,
    desired: &DesiredChange,
) -> Result<ReadDeclaration> {
    let mut declaration = capture::capability_reads()?;
    for artifact in &change.files {
        declaration = declaration.file(relative_path(project, &artifact.path)?);
    }
    for resource in &desired.resources {
        if let ResourceKey::SpringTestImport { path, .. } = &resource.key {
            declaration = declaration.file(path.clone());
        }
    }
    Ok(declaration)
}

/// A planned artifact's path, as the project-relative name a resource has.
///
/// A recipe plans in absolute paths because it writes files; a resource is
/// named by where it sits in the project, so that the same record means the
/// same thing on another machine.
fn relative_path(project: &Project, path: &std::path::Path) -> Result<ProjectPath> {
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
    // Read once, and let the same value decide the generation the plan claims
    // and the image the commit guards under the lock. Reading them apart is
    // how a plan comes to be written against a store that moved in between.
    let observed = observed(project)?;
    let set = request.against(&observed)?;
    commit_set(project, set, declaration, description)
}

/// The same steps, for a request that already knows what the store becomes.
///
/// A one-shot does not go through [`Request`]: there is no ownership to
/// reconcile, so there is nothing to measure against the store. It states its
/// receipt and its file and that is the whole transition.
fn commit_set(
    project: &Project,
    set: DesiredChangeSet,
    declaration: &ReadDeclaration,
    description: &str,
) -> Result<CommitResult> {
    let (snapshot, mut projection) = capture::projected(project, declaration)?;
    let observed = observed(project)?;
    if let Some(store) = &observed.ledger {
        projection.record(&store.resources);
    }
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
