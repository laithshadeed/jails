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
use jails_prepare::command::{CommandEnvelope, ProjectCommitDisposition};
use jails_prepare::desire;
use jails_prepare::pipeline::{self, ObservedStore, PreparationContext};
use jails_prepare::recovery::RecoveryOutcome;
use jails_prepare::report::{Report, ReportedOp};
use jails_project::capability::Declaration;
use jails_project::capture::{self, ReadDeclaration};
use jails_project::model::{Change, Project};
use jails_protocol::bootstrap::Bootstrap;
use jails_protocol::change::{DesiredChange, MaintenanceAttribution};
use jails_protocol::context::RenderedSubjectContext;
use jails_protocol::declaration::{FieldSpec, IntentArguments, IntentSpec};
use jails_protocol::edit::SemanticEdit;
use jails_protocol::entity::{
    CapabilityId, CapabilitySpec, EntityId, EntitySpec, IntentId, OneShotId, OneShotSpec, OwnerId,
    SourceInputId,
};
use jails_protocol::identity::{JavaType, Name, ObjectId, Package, ProjectPath};
use jails_protocol::ownership::{DesiredEntity, DesiredState, ObservedEntity, ReconcileScope};
use jails_protocol::pending::{DesiredInputGuard, DesiredInputId, FrozenDesiredInput};
use jails_protocol::plan::{
    DesiredAppliedEntity, DesiredChangeSet, DesiredOneShotReceipt, LedgerIntent, PlannedSubject,
};
use jails_protocol::provenance::{OneShotKind, RendererId};
use jails_protocol::render::{DesiredBody, DesiredFile, ManagedPath};
use jails_protocol::request::{
    CanonicalCapability, CanonicalGenerateRequest, CanonicalMutationRequest,
    CanonicalRequestSyntaxV1,
};
use jails_protocol::resource::{
    DesiredResource, OneShotLifecycle, OneShotState, ResourceKey, ResourceOwner, ResourceValue,
};
use jails_protocol::snapshot::{MachineRootPresence, TemplateStore};
use jails_protocol::transition::CommitPlan;
use jails_spec::spec::kind::{ArtifactKind, Capability};
use jails_support::Result;

mod app;
mod artifact;
mod capability;
mod feature;
mod field;
mod maintenance;
mod oneshot;
mod provenance;
mod session;

pub use app::{Intent, app_apply};
pub use artifact::{destroy, generate, recipe};
pub use capability::{install, remove, sync};
pub use feature::{install_fast_test, remove_fast_test};
pub use field::field;
pub use maintenance::{adopt_layout, adopt_legacy, app_init, format, rename};
pub use oneshot::{cases, migration};
pub use session::{Outcome, Run};

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
    // A Spring project with compose services gets the module that starts them
    // for `spring-boot:run`. Same rule as the three above and the same reason
    // it lives here: the recipes that bring a service are `db`, `kafka`,
    // `redis` and `mail`, and a rule four of them have to remember is one that
    // decays. `add db` also writes `spring.docker.compose.enabled=false` on
    // this machine, and both are right: the property is a per-project answer
    // to a broken provider, and the dependency is what the property is *about*.
    if project.flavor() == jails_project::pom::Flavor::SpringBoot
        && !change.compose.is_empty()
        && !jails_project::pom::has_dependency(
            project.pom(),
            "org.springframework.boot",
            "spring-boot-docker-compose",
        )
    {
        change.deps.push(jails_project::pom::SPRING_DOCKER_COMPOSE);
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
    spec: &CapabilitySpec,
) -> Result<()> {
    let key = ResourceKey::HumanConfigCapability(id.clone());
    let spec = spec.clone();
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
            ResourceKey::SpringTestImport { path, .. } | ResourceKey::MarkedBlock { path, .. } => {
                declaration = declaration.file(path.clone())
            }
            ResourceKey::CommandRegistration { dispatcher, .. } => {
                declaration = declaration.file(dispatcher_source(dispatcher)?)
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
/// `Clone` because §R3.4's replan-once loop measures the same request against
/// the store twice when recovery moves it in between -- the *request* is what
/// was asked and does not change; only what the store makes of it does.
///
/// Deliberately not a `DesiredChangeSet` yet. That value states what the store
/// looks like afterwards, and afterwards is a function of what is there now --
/// which exactly one place may read (see [`commit`]). A field filled with a
/// placeholder here and corrected there is two authorities on one number, and
/// the executor refuses when they disagree.
#[derive(Clone)]
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
        match &resource.key {
            // A surgical edit is made *in* a file this change does not own, so
            // the file is a precondition of the plan rather than an incidental
            // read: the executor rechecks it under the lock, and a block
            // spliced into bytes that moved in between would be spliced into
            // the wrong place.
            ResourceKey::SpringTestImport { path, .. } | ResourceKey::MarkedBlock { path, .. } => {
                declaration = declaration.file(path.clone());
            }
            ResourceKey::CommandRegistration { dispatcher, .. } => {
                declaration = declaration.file(dispatcher_source(dispatcher)?);
            }
            _ => {}
        }
    }
    Ok(declaration)
}

/// Where a dispatcher's source lives, by the convention every Java build
/// follows. The same derivation the projection uses, so a read and the splice
/// that depends on it cannot name two files.
fn dispatcher_source(ty: &jails_protocol::identity::JavaType) -> Result<ProjectPath> {
    let package = ty.package();
    let directory = match package.is_base() {
        true => String::new(),
        false => format!("{}/", package.as_str().replace('.', "/")),
    };
    ProjectPath::parse(&format!("src/main/java/{directory}{}.java", ty.name()))
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
///
/// Replans exactly once, and §R3.4 says why once: recovery may have finished
/// an interrupted earlier transaction between the read and the lock, which
/// makes this plan describe a store that has moved. That is not an error --
/// nothing is wrong, the plan is simply stale -- so the store is reread and
/// the request measured against it again. A *second* such answer is
/// `RecoveryBlocked` rather than a third attempt: recovery changing
/// authoritative state twice in one invocation is a loop, not a race.
fn commit(
    run: &Run,
    request: Request,
    declaration: &ReadDeclaration,
    asked: &Asked,
) -> Result<Outcome> {
    let project = run.project();
    let mut recovery = Vec::new();
    for attempt in 0..2 {
        // Read once per attempt, and let the same value decide the generation
        // the plan claims and the image the commit guards under the lock.
        // Reading them apart is how a plan comes to be written against a store
        // that moved in between.
        let observed = observed(project)?;
        let set = request.clone().against(&observed)?;
        let outcome = commit_set(run, set, declaration, asked)?;
        match outcome.replanned() {
            Some(outcome) if attempt == 0 => recovery.push(outcome),
            Some(_) => {
                return Err(
                    "recovery changed this project twice while one command was running.\n                            fix: run `jails doctor`. Replanning again would be a loop rather than a \
                     race, so jails stops and says so instead."
                        .to_string(),
                );
            }
            None => return Ok(outcome.after_recovery(recovery)),
        }
    }
    unreachable!("the loop returns on every path")
}

/// The same steps, for a request that already knows what the store becomes.
///
/// A one-shot does not go through [`Request`]: there is no ownership to
/// reconcile, so there is nothing to measure against the store. It states its
/// receipt and its file and that is the whole transition.
fn commit_set(
    run: &Run,
    set: DesiredChangeSet,
    declaration: &ReadDeclaration,
    asked: &Asked,
) -> Result<Outcome> {
    let project = run.project();
    let bundle = prepare_set(run, set, declaration, Some(asked))?;
    if !run.write {
        return Ok(Outcome::Planned(Box::new(
            jails_prepare::report::Report::of(&bundle.change)?,
        )));
    }
    let handle = ProjectHandle::at(project.root())?;
    let locked = LockedProject::acquire(handle, &asked.display()).map_err(describe)?;
    let result = execute::commit(&locked, &bundle).map_err(describe)?;
    // The project lock goes *before* the runtime reconciliation, which is
    // §R6.6's rule and not an optimisation: `docker compose up -d` can take a
    // minute pulling an image, and holding the mutation lock across it would
    // make every other jails command in the tree wait on a container.
    drop(locked);
    Ok(Outcome::Committed(reconciled(run, result)?))
}

/// Attempt the effect the commit recorded, if it recorded one.
///
/// The commit is already durable when this runs. A failed attempt is
/// therefore reported, never unwound: the project is in the state it was
/// asked for, and what is missing is a container -- which the receipt now
/// carries a retryable descriptor for.
fn reconciled(run: &Run, result: CommitResult) -> Result<CommitResult> {
    let CommitResult::Committed(committed) = result else {
        return Ok(result);
    };
    if committed.receipt.post_commit.is_empty() {
        return Ok(CommitResult::Committed(committed));
    }
    let store = jails_commit::store::Store::at(run.project().root());
    let effect = jails_commit::runtime::reconcile(
        &store,
        run.project().root(),
        &committed.receipt.transaction_id,
        run.debug,
    )
    .map_err(describe)?;
    Ok(CommitResult::Committed(Box::new(
        jails_commit::outcome::CommittedResult {
            effect,
            ..*committed
        },
    )))
}

/// Everything a commit does except taking the lock and activating.
///
/// A plan is not a weaker commit that stops early by accident -- it is the
/// same computation, and the bundle it produces is the *exact* one the commit
/// would have activated. Anything that describes a transition therefore
/// describes this value, which is what makes `--pretend` an answer about what
/// will happen rather than a second implementation that hopes to agree.
fn prepare_set(
    run: &Run,
    set: DesiredChangeSet,
    declaration: &ReadDeclaration,
    asked: Option<&Asked>,
) -> Result<pipeline::PreparedBundle> {
    let project = run.project();
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
        claimed: run.claimed.clone(),
        // Derived from the canonical request, not from the flag alone: §R3.3
        // makes every request variant without a `no_start` field ineligible
        // and says it behaves as `no_start == true`, so a maintenance action
        // cannot reconcile a runtime by accident.
        start_services: asked.is_some_and(Asked::starts_services),
        // Computed against the same capture the plan was, so the row for
        // `jails.toml` describes the bytes this plan actually read rather
        // than whatever is on disk by the time it is asked for.
        invocation: match asked {
            Some(asked) => Some(asked.fingerprint(&snapshot)?),
            None => None,
        },
        // The durable object store, as the one question preparation asks of
        // it: given a recorded base, the bytes jails wrote. A three-way merge
        // measures the reader's edit and the generator's change from exactly
        // those, and there is nowhere else to get them -- the file on disk is
        // one of the two sides, not the origin.
        objects: {
            let at = jails_commit::store::Store::at(project.root()).objects();
            std::sync::Arc::new(move |id: &jails_protocol::identity::ObjectId| {
                jails_commit::store::read_object(&at, id).ok()
            })
        },
    };
    pipeline::prepare(
        &loaded,
        CommitPlan::Apply(set),
        snapshot,
        projection,
        context,
    )
}

/// What was asked for, canonically -- both halves of §R5.4's invocation.
///
/// The two are not redundant. `request` is the *meaning*: which capabilities,
/// which recipe, which force flag, with aliases resolved and set-valued
/// positions sorted. `syntax` is the *spelling*, and it is what a resume
/// compares first, because two different spellings of one meaning are still
/// two different things a person typed and a resumption that silently
/// accepted either would be resuming the wrong one.
///
/// Built by the route rather than parsed out of `argv`. A route knows what it
/// was asked far more exactly than a parser reading the command line back
/// does, and there is no second implementation to disagree with.
pub struct Asked {
    request: CanonicalMutationRequest,
    syntax: CanonicalRequestSyntaxV1,
}

impl Asked {
    /// Name the command and the arguments that decide what it does.
    ///
    /// `command` is the subcommand path without dashes; `positionals` are its
    /// arguments; `options`/`flags` carry only what was explicitly supplied
    /// and only what is *semantic* -- §R5.4 excludes presentation flags
    /// (`--debug`, an output format) because rerunning with colour on is the
    /// same request.
    pub fn new(
        request: CanonicalMutationRequest,
        command: &[&str],
        positionals: Vec<String>,
        options: BTreeMap<String, Vec<String>>,
        flags: BTreeSet<String>,
    ) -> Self {
        Self {
            request,
            syntax: CanonicalRequestSyntaxV1 {
                command_path: command.iter().map(|part| part.to_string()).collect(),
                positionals,
                options,
                flags,
            },
        }
    }

    /// The shorter form: a command with positional arguments and nothing else.
    pub fn plain(
        request: CanonicalMutationRequest,
        command: &[&str],
        positionals: &[&str],
    ) -> Self {
        Self::new(
            request,
            command,
            positionals.iter().map(|one| one.to_string()).collect(),
            BTreeMap::new(),
            BTreeSet::new(),
        )
    }

    /// The line a lock, a report and a resume prompt all show.
    fn display(&self) -> String {
        let mut out = String::from("jails");
        for part in self
            .syntax
            .command_path
            .iter()
            .chain(self.syntax.positionals.iter())
        {
            out.push(' ');
            out.push_str(part);
        }
        out
    }

    /// Whether this request is one that may reconcile runtime services.
    ///
    /// §R3.3: only the variants carrying a `no_start` field are eligible, and
    /// every other one behaves as though it had said `--no-start`. Read off
    /// the canonical request rather than off a flag, so the answer is a
    /// property of what was asked rather than of what a caller remembered to
    /// pass.
    fn starts_services(&self) -> bool {
        match &self.request {
            CanonicalMutationRequest::Add { no_start, .. }
            | CanonicalMutationRequest::Remove { no_start, .. }
            | CanonicalMutationRequest::Sync { no_start }
            | CanonicalMutationRequest::AppApply { no_start } => !no_start,
            _ => false,
        }
    }

    /// §R5.4's fingerprint, over this request and the human inputs it reads.
    ///
    /// `DirectRequest` is mandatory and hashes the request's own canonical
    /// bytes, which is what makes the fingerprint depend on *what was asked*
    /// rather than only on how it was spelled. The other rows are the human
    /// sources a resumption must find unchanged; `jails.toml` is the one every
    /// route may touch, and its absence is a row too -- "there was no config"
    /// is a fact a resume has to be able to check, not a gap.
    fn fingerprint(
        &self,
        snapshot: &jails_protocol::snapshot::ProjectSnapshot,
    ) -> Result<jails_protocol::request::InvocationFingerprint> {
        let mut rows = vec![FrozenDesiredInput {
            id: DesiredInputId::DirectRequest,
            guard: {
                let mut encoder = jails_support::codec::Encoder::new();
                self.request.encode(&mut encoder)?;
                let bytes = encoder.finish()?;
                DesiredInputGuard::Exact {
                    sha256: ObjectId::from_bytes(jails_support::codec::sha256(&bytes)),
                    len: bytes.len() as u64,
                }
            },
        }];
        let config = ProjectPath::parse(jails_project::config::FILE)?;
        rows.push(FrozenDesiredInput {
            id: DesiredInputId::HumanConfig,
            guard: match snapshot.read(&config)? {
                jails_protocol::snapshot::Captured::Present(file) => DesiredInputGuard::Exact {
                    sha256: ObjectId::from_bytes(jails_support::codec::sha256(&file.bytes)),
                    len: file.bytes.len() as u64,
                },
                jails_protocol::snapshot::Captured::Absent => DesiredInputGuard::Absent,
            },
        });
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        let mut encoder = jails_support::codec::Encoder::new();
        encoder.count(rows.len())?;
        for row in &rows {
            row.encode(&mut encoder)?;
        }
        Ok(jails_protocol::request::InvocationFingerprint {
            request_syntax: self.syntax.fingerprint()?,
            request: self.request.clone(),
            // Direct CLI: no manifest is the source. `app apply` overrides
            // this once a manifest identity is threaded through.
            manifest_source: None,
            desired_input_sha256: ObjectId::from_bytes(jails_support::codec::domain_hash(
                "JAILS-DESIRED-INPUT-1",
                &encoder.finish()?,
            )),
        })
    }
}

/// The `Asked` for a command whose whole argument is one capability.
///
/// The three capability routes share it because they share the shape: one
/// name, spelled as `Capability::label()` rather than whatever alias was
/// typed, so `jails add postgres` and `jails add db` are recognised as the
/// same request by anything comparing fingerprints.
/// The canonical syntax of a capability command, parameters included.
///
/// A fingerprint proves two invocations are the same command, so `add csv
/// --name Order` and `add csv --name Invoice` must not render the same
/// syntax. Built here from the resolved declaration rather than re-parsed out
/// of `argv`, for the reason §R6.1 gives: a route knows what it was asked far
/// more exactly than a re-parse does, and there is no second implementation to
/// disagree with.
fn asked_capabilities(
    command: &[&str],
    declaration: &Declaration,
    request: CanonicalMutationRequest,
) -> Asked {
    let mut syntax: Vec<String> = vec![declaration.kind.label().to_string()];
    if let Some(name) = &declaration.name {
        syntax.push("--name".to_string());
        syntax.push(name.clone());
    }
    if let Some(package) = &declaration.package {
        syntax.push("--package".to_string());
        syntax.push(package.clone());
    }
    let syntax: Vec<&str> = syntax.iter().map(String::as_str).collect();
    Asked::plain(request, command, &syntax)
}

/// A commit failure as the one line a person reads.
///
/// Every one of these is a refusal before anything was activated -- that is
/// what `CommitError` *means*, and it is why this is a plain message rather
/// than a recovery instruction.
fn describe(error: CommitError) -> String {
    format!("{error:?}")
}
