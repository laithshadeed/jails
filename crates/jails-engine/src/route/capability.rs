//! `add`, `sync` and `remove`: the routes whose subject is the capability list.
//!
//! All three speak for `jails.toml`'s `[project] capabilities`, which is what
//! `ReconcileScope::DirectConfig` names -- so all three declare the *whole*
//! list and let reconciliation work out the difference. That is why `remove`
//! describes nothing and `sync` is one transition rather than a loop.

use super::*;

/// Install one capability through the transaction protocol.
///
/// The direct counterpart of `add::add_in`, and deliberately the same subject:
/// `ReconcileScope::DirectConfig` speaks for the capability list in
/// `jails.toml`, which is what `sync` later reconciles against.
///
/// `--name` and `--package` are not passed through to the recipe and forgotten:
/// they decide *which* capability this is. `add csv --name Order` and `add csv
/// --name Invoice` are two, and a singleton identity would make the second look
/// like the first already installed. Which parameters a capability accepts is
/// [`jails_project::capability::identity`]'s to refuse, so a parameter that has
/// no meaning is reported rather than quietly dropped by a recipe that happens
/// not to read it.
pub fn install(run: &Run, asked: &Declaration) -> Result<Outcome> {
    let project = run.project();
    let capability = asked.kind;
    // Not exempted, on purpose, and this is the one route where the rule bites:
    // a capability is a dependency plus code plus a test, and jails will not
    // edit a build file it refuses to read. Installing the code and silently
    // skipping the dependency hands the reader a compile error for a line they
    // did not write. `generate` degrades instead -- it emits Java that assumes
    // the plainer shape and says so -- because about ten of thirty commands
    // need Maven at all and refusing the rest would refuse a foreign project
    // for no reason.
    project.require_maven(capability.label())?;
    let (id, spec) = asked.resolve(project)?;
    let declared_as = Declaration::of(&id, &spec);
    let owner = ResourceOwner::Entity(EntityId::Capability(id.clone()));
    let change = with_test_support(
        project,
        jails_generate::add::plan_named(
            capability,
            project,
            asked.name.as_deref(),
            asked.package.as_deref(),
        )?,
    );
    let mut desired = desire::contribution(&owner, &change, project)?;
    record_capability(&mut desired, &owner, &id, &spec)?;
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Capability(capability),
        Some(RenderedSubjectContext::Entity {
            id: EntityId::Capability(id.clone()),
            spec: EntitySpec::Capability(spec.clone()),
        }),
    )?;
    let entity = DesiredEntity {
        id: EntityId::Capability(id.clone()),
        spec: EntitySpec::Capability(spec.clone()),
        owners: BTreeSet::from([OwnerId::DirectConfig]),
    };
    let reads = declaration(project, &change, &desired)?;
    let request = Request {
        scope: ReconcileScope::DirectConfig,
        declared: declared_capabilities(&observed(project)?, Some(entity))?,
        changes: vec![desired],
    };
    commit(
        run,
        request,
        &reads,
        &asked_capabilities(
            &["add"],
            &declared_as,
            CanonicalMutationRequest::Add {
                capabilities: CanonicalMutationRequest::capabilities(vec![CanonicalCapability {
                    id,
                    spec,
                }])?,
                no_start: run.no_start(),
            },
        ),
    )
}

/// Make the project match the capability list in `jails.toml`.
///
/// One transition, not a loop of installs: everything the manifest names and
/// nothing it does not, decided in one reconciliation. A capability listed but
/// not installed arrives; one installed but no longer listed leaves; and the
/// two happen together or not at all, which is what stops a half-applied sync
/// from leaving a project neither state.
///
/// The manifest is the authority here, not the store. That is the whole point
/// of `sync`: somebody edited the list, and this is how the project catches up.
pub fn sync(run: &Run) -> Result<Outcome> {
    let project = run.project();
    if !project.declarations().is_empty() {
        project.require_maven("sync")?;
    }
    let store = observed(project)?;
    let mut declared = BTreeMap::new();
    let mut changes = Vec::new();
    let mut reads = capture::capability_reads()?;
    // The declarations, not the labels: a `[[capability]]` table carries the
    // `--name`/`--package` that decide which capability its row is, and
    // reconstructing a singleton from the label alone would declare a
    // different entity from the one `add` recorded -- so this transition would
    // retire the named row it was meant to keep.
    for declaration in project.declarations() {
        let capability = declaration.kind;
        let (id, spec) = declaration.resolve(project)?;
        let owner = ResourceOwner::Entity(EntityId::Capability(id.clone()));
        declared.insert(
            EntityId::Capability(id.clone()),
            DesiredEntity {
                id: EntityId::Capability(id.clone()),
                spec: EntitySpec::Capability(spec.clone()),
                owners: BTreeSet::from([OwnerId::DirectConfig]),
            },
        );
        // Already recorded means already installed, and re-planning it would
        // ask the recipe to describe a project it has already changed.
        if store.ledger.as_ref().is_some_and(|ledger| {
            ledger
                .applied
                .iter()
                .any(|row| row.id == EntityId::Capability(id.clone()))
        }) {
            continue;
        }
        let change = with_test_support(
            project,
            jails_generate::add::plan_named(
                capability,
                project,
                declaration.name.as_deref(),
                declaration.package.as_deref(),
            )?,
        );
        let mut desired = desire::contribution(&owner, &change, project)?;
        record_capability(&mut desired, &owner, &id, &spec)?;
        provenance::stamp_files(
            &mut desired,
            project,
            RendererId::Capability(capability),
            Some(RenderedSubjectContext::Entity {
                id: EntityId::Capability(id.clone()),
                spec: EntitySpec::Capability(spec.clone()),
            }),
        )?;
        for artifact in &change.files {
            reads = reads.file(relative_path(project, &artifact.path)?);
        }
        // Same rule as `install`: a file this capability edits surgically is
        // a precondition of the plan, not an incidental read.
        for resource in &desired.resources {
            if let ResourceKey::SpringTestImport { path, .. } = &resource.key {
                reads = reads.file(path.clone());
            }
        }
        changes.push(desired);
    }
    for row in store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
    {
        if let ResourceKey::WholeFile(path) = &row.key {
            reads = reads.file(path.clone());
        }
    }

    let request = Request {
        scope: ReconcileScope::DirectConfig,
        declared,
        changes,
    };
    commit(
        run,
        request,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::Sync {
                no_start: run.no_start(),
            },
            &["sync"],
            &[],
        ),
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
pub fn remove(run: &Run, asked: &Declaration) -> Result<Outcome> {
    let project = run.project();
    let (id, spec) = asked.resolve(project)?;
    let declared_as = Declaration::of(&id, &spec);
    let entity = EntityId::Capability(id.clone());
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
            declared_as.display()
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
        changes: Vec::new(),
    };
    commit(
        run,
        request,
        &retiring(&store, &owner)?,
        &asked_capabilities(
            &["remove"],
            &declared_as,
            CanonicalMutationRequest::Remove {
                capabilities: CanonicalMutationRequest::capabilities(vec![CanonicalCapability {
                    id,
                    spec,
                }])?,
                force: false,
                no_start: run.no_start(),
            },
        ),
    )
}
