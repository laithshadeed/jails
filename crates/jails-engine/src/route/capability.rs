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
pub fn install(run: &Run, capability: Capability) -> Result<Outcome> {
    let project = run.project();
    let id = CapabilityId {
        kind: capability,
        instance: CapabilityInstance::Singleton,
    };
    let owner = ResourceOwner::Entity(EntityId::Capability(id.clone()));
    let change = with_test_support(project, jails_generate::add::plan_for(capability, project)?);
    let mut desired = desire::contribution(&owner, &change, project)?;
    record_capability(&mut desired, &owner, &id)?;
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Capability(capability),
        Some(RenderedSubjectContext::Entity {
            id: EntityId::Capability(id.clone()),
            spec: EntitySpec::Capability(CapabilitySpec { placement: None }),
        }),
    )?;
    let entity = DesiredEntity {
        id: EntityId::Capability(id),
        spec: EntitySpec::Capability(CapabilitySpec { placement: None }),
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
            capability,
            CanonicalMutationRequest::Add {
                capabilities: CanonicalMutationRequest::capabilities(vec![CanonicalCapability {
                    id: CapabilityId {
                        kind: capability,
                        instance: CapabilityInstance::Singleton,
                    },
                    spec: CapabilitySpec { placement: None },
                }])?,
                no_start: false,
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
    let store = observed(project)?;
    let mut declared = BTreeMap::new();
    let mut changes = Vec::new();
    let mut reads = capture::capability_reads()?;
    for label in project.capabilities() {
        // The manifest stores `Capability::label()` spellings, never clap
        // aliases -- CLAUDE.md's rule, so that one capability cannot be listed
        // twice under two names. An unknown one is an error rather than a
        // skipped line: silently ignoring it would make `sync` report success
        // over a project it did not finish.
        let capability = Capability::value_variants()
            .iter()
            .copied()
            .find(|candidate| candidate.label() == label)
            .ok_or_else(|| {
                format!(
                    "`{label}` in jails.toml is not a capability this jails knows.\n       fix: \
                     run `jails commands --json` for the list, or use a newer jails."
                )
            })?;
        let id = CapabilityId {
            kind: capability,
            instance: CapabilityInstance::Singleton,
        };
        let owner = ResourceOwner::Entity(EntityId::Capability(id.clone()));
        declared.insert(
            EntityId::Capability(id.clone()),
            DesiredEntity {
                id: EntityId::Capability(id.clone()),
                spec: EntitySpec::Capability(CapabilitySpec { placement: None }),
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
        let change =
            with_test_support(project, jails_generate::add::plan_for(capability, project)?);
        let mut desired = desire::contribution(&owner, &change, project)?;
        record_capability(&mut desired, &owner, &id)?;
        provenance::stamp_files(
            &mut desired,
            project,
            RendererId::Capability(capability),
            Some(RenderedSubjectContext::Entity {
                id: EntityId::Capability(id.clone()),
                spec: EntitySpec::Capability(CapabilitySpec { placement: None }),
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
            CanonicalMutationRequest::Sync { no_start: false },
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
pub fn remove(run: &Run, capability: Capability) -> Result<Outcome> {
    let project = run.project();
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
        changes: Vec::new(),
    };
    commit(
        run,
        request,
        &retiring(&store, &owner)?,
        &asked_capabilities(
            &["remove"],
            capability,
            CanonicalMutationRequest::Remove {
                capabilities: CanonicalMutationRequest::capabilities(vec![CanonicalCapability {
                    id: CapabilityId {
                        kind: capability,
                        instance: CapabilityInstance::Singleton,
                    },
                    spec: CapabilitySpec { placement: None },
                }])?,
                force: false,
                no_start: false,
            },
        ),
    )
}
