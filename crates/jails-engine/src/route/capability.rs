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
    // The project has to be able to *compile* what this installs. Checked
    // here rather than at the dispatch point, because it is a property of the
    // project a capability plans against and every caller of this route --
    // `add`, `sync`, and an aggregate `app apply` -- needs it equally.
    jails_generate::add::require_java_release(project.build(), project.java_release())?;
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
    let installed = commit(
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
    )?;
    note_unstarted(
        run,
        &change
            .compose
            .iter()
            .map(|service| service.name)
            .collect::<Vec<_>>(),
    );
    reformat_after(run, matches!(capability, Capability::Format), installed)
}

/// Installing a formatter leaves the project failing its own `verify`, unless
/// the formatter runs.
///
/// A formatter has an opinion about line wrapping that no amount of careful
/// templating can predict, so the only way to hand back a project that passes
/// `jails check` is to actually run it once over what is already there.
///
/// **Two transitions, deliberately.** V1 shelled out to `spotless:apply` after
/// its own write path, which is precisely the shape this migration exists to
/// remove: a write the routes do not know about. Here the reformat is
/// `route::format` -- the same transition `jails fmt` is -- so it runs in a
/// scratch tree synthesised from the projection, declares its mutable scopes,
/// and commits only what it changed. It cannot happen inside the install,
/// because the plugin it needs is what the install just put in the pom.
///
/// Best-effort on the toolchain, not on the transition: a machine with no
/// Maven gets the capability and a first `jails fmt` that has work to do,
/// which is what V1 promised too.
pub(super) fn reformat_after(run: &Run, formats: bool, installed: Outcome) -> Result<Outcome> {
    if !formats || !run.writes() {
        return Ok(installed);
    }
    match super::format(run) {
        Ok(_) => Ok(installed),
        // The capability is installed and recorded; only the one-off pass over
        // existing sources did not happen. Failing here would roll nothing
        // back -- the commit is already published -- so it would report a
        // failure over a project that is in exactly the state it asked for.
        Err(_) => Ok(installed),
    }
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
    let mut unstarted: Vec<&str> = Vec::new();
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
        // Every declared capability is re-planned, installed or not. Skipping
        // the recorded ones made `sync` mean "install what is missing" rather
        // than "make the project match the list", and the difference is not
        // academic: a capability wires itself into what the project has, so
        // `add db`'s import of its container config never reached a
        // `@SpringBootTest` written after it -- the project came out with a
        // test that has no DataSource and fails on a test nobody wrote.
        //
        // Safe because planning is pure and its paths are functions of the
        // entity rather than of how many times it has been planned: an
        // unchanged capability produces an identical desired set, and the
        // reconciler turns that into no operations at all.
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
            if let ResourceKey::SpringTestImport { path, .. }
            | ResourceKey::MarkedBlock { path, .. } = &resource.key
            {
                reads = reads.file(path.clone());
            }
        }
        unstarted.extend(change.compose.iter().map(|service| service.name));
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

    let formats = declared
        .keys()
        .any(|id| matches!(id, EntityId::Capability(id) if id.kind == Capability::Format));
    let request = Request {
        scope: ReconcileScope::DirectConfig,
        declared,
        changes,
    };
    let synced = commit(
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
    )?;
    note_unstarted(run, &unstarted);
    reformat_after(run, formats, synced)
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

/// Say what this transition committed and deliberately did not start.
///
/// `--no-start` makes the compose effect ineligible at preparation -- §R3.3
/// gives the descriptor no existence at all rather than an unattempted one --
/// so nothing downstream can report the services as pending. Without this the
/// reader gets a `compose.yaml` with a database in it and no word that the
/// database is not running, which is the failure they meet at the next `mvn
/// verify` rather than here.
///
/// Read off the plan's own service list, so it names the capability's command
/// (`jails start db`) rather than a bare start of everything the file happens
/// to declare.
fn note_unstarted(run: &Run, services: &[&str]) {
    if !run.no_start() || !run.writes() || services.is_empty() {
        return;
    }
    println!(
        "  note    start with `{}`",
        jails_project::compose::missing_docker_hint(services)
    );
}
