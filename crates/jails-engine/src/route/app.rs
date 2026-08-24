//! `app apply`: a whole manifest as one transition.
//!
//! §R6.2's `app::apply` row asks for "one aggregate projected plan and one
//! commit", and names what that deletes: the project reload after every
//! capability and intent, the per-intent ledger save, the second capability
//! pass, and partial success.
//!
//! ## Why the reload existed, and what replaces it
//!
//! A manifest is ordered because its steps depend on each other. `add db`
//! puts the JDBC starter in the POM and `g search` refuses without it;
//! `g scaffold Note` writes a record and `g search Note title` reads its
//! components back. V1 makes that work by writing each step to disk and
//! resolving the project again, which is why a failure halfway leaves a
//! project that is neither the old one nor the new one.
//!
//! Here each step plans against a **projection** of everything before it.
//! `Project::projected` is the same value the recipes already take, resolved
//! from planned bytes instead of from disk -- so nothing about a recipe
//! changes, and nothing is written until the whole manifest has planned.
//!
//! The projection is rebuilt per step rather than grown in place, because the
//! read declaration is only known once a step has planned: a capture may not
//! be asked for a path nobody declared. That is a handful of small reads per
//! manifest entry, and the alternative is a plan that reaches past its own
//! snapshot.

use super::*;

/// One `[[generate]]` row, as the engine takes it.
///
/// Deliberately not `app::ResolvedIntent`: that type lives in the binary,
/// which is above this crate, and it carries manifest syntax -- deprecated
/// aliases, the `timestamps` flag that is expanded before a recipe sees it --
/// that a route has no business knowing about.
#[derive(Clone, Debug)]
pub struct AppIntent {
    pub kind: ArtifactKind,
    pub name: String,
    pub fields: Vec<String>,
    /// Expanded into two ordinary components before anything plans, through
    /// the same helper the CLI flag uses.
    pub timestamps: bool,
    pub indexes: Vec<String>,
    pub package: Option<String>,
    pub on: Option<String>,
    pub yields: Option<String>,
}

/// Apply a whole manifest in one transition.
///
/// `Run::pretending` makes this `app plan`, and there is deliberately no
/// second function for it. V1 answers `app plan` with a separate walk over the
/// intent list that compares each row against the ledger and prints
/// `pending`/`update`/`applied` -- a walk that cannot see a file the reader
/// edited, cannot tell a regeneration that changes nothing from one that
/// rewrites a class, and had to be shadowed against a typed comparison
/// precisely because two implementations of one question disagree. Here the
/// plan is this computation stopped one step before the lock: what it names is
/// exactly what an apply then writes.
pub fn app_apply(run: &Run, capabilities: &[Capability], intents: &[AppIntent]) -> Result<Outcome> {
    let project = run.project();
    let (request, reads) = declare(project, capabilities, intents)?;
    commit(
        run,
        request,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::AppApply { no_start: false },
            &["app", "apply"],
            &[],
        ),
    )
}

/// The manifest as a request, and everything reading it declared.
fn declare(
    project: &Project,
    capabilities: &[Capability],
    intents: &[AppIntent],
) -> Result<(Request, ReadDeclaration)> {
    let store = observed(project)?;
    let mut declared: BTreeMap<EntityId, DesiredEntity> = BTreeMap::new();
    let mut changes: Vec<DesiredChange> = Vec::new();
    let mut reads = capture::capability_reads()?;

    for &capability in capabilities {
        let planned = projected(project, &reads, &changes)?;
        let id = CapabilityId {
            kind: capability,
            instance: CapabilityInstance::Singleton,
        };
        let owner = ResourceOwner::Entity(EntityId::Capability(id.clone()));
        let change = with_test_support(
            &planned,
            jails_generate::add::plan_for(capability, &planned)?,
        );
        let mut desired = desire::contribution(&owner, &change, &planned)?;
        record_capability(&mut desired, &owner, &id)?;
        provenance::stamp_files(
            &mut desired,
            &planned,
            RendererId::Capability(capability),
            Some(RenderedSubjectContext::Entity {
                id: EntityId::Capability(id.clone()),
                spec: EntitySpec::Capability(CapabilitySpec { placement: None }),
            }),
        )?;
        declared.insert(
            EntityId::Capability(id.clone()),
            DesiredEntity {
                id: EntityId::Capability(id),
                spec: EntitySpec::Capability(CapabilitySpec { placement: None }),
                // The manifest is the owner, not `jails.toml`'s list: absence
                // *there* really does mean relinquished, which is what makes
                // a removed row a removal rather than silence.
                owners: BTreeSet::from([OwnerId::AppManifest]),
            },
        );
        reads = widen(reads, &planned, &change, &desired)?;
        changes.push(desired);
    }

    for intent in intents {
        let planned = projected(project, &reads, &changes)?;
        let expanded;
        let fields = match intent.timestamps {
            true => {
                expanded = jails_generate::generate::with_timestamps(intent.kind, &intent.fields)?;
                expanded.as_slice()
            }
            false => intent.fields.as_slice(),
        };
        let change = with_test_support(
            &planned,
            jails_generate::generate::plan_recipe(
                &planned,
                intent.kind,
                &intent.name,
                fields,
                intent.package.as_deref(),
                &intent.indexes,
                intent.on.as_deref(),
                intent.yields.as_deref(),
            )?,
        );
        let id = super::intent(
            &planned,
            intent.kind,
            &intent.name,
            intent.package.as_deref(),
            fields,
            &intent.indexes,
            intent.on.as_deref(),
            intent.yields.as_deref(),
        )?;
        let spec = super::spec(
            &planned,
            intent.kind,
            fields,
            &intent.indexes,
            intent.on.as_deref(),
            intent.yields.as_deref(),
        )?;
        let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
        let mut desired = desire::contribution(&owner, &change, &planned)?;
        provenance::stamp_files(
            &mut desired,
            &planned,
            RendererId::Recipe(intent.kind),
            Some(RenderedSubjectContext::Entity {
                id: EntityId::Intent(id.clone()),
                spec: EntitySpec::Intent(spec.clone()),
            }),
        )?;
        if declared
            .insert(
                EntityId::Intent(id.clone()),
                DesiredEntity {
                    id: EntityId::Intent(id.clone()),
                    spec: EntitySpec::Intent(spec),
                    owners: BTreeSet::from([OwnerId::AppManifest]),
                },
            )
            .is_some()
        {
            return Err(format!(
                "the manifest declares `{} {}` twice.\n       fix: one row per artifact. Two \
                 rows for one identity would apply both, and the second would land on the \
                 first's files.",
                label(intent.kind),
                id.name
            ));
        }
        reads = widen(reads, &planned, &change, &desired)?;
        changes.push(desired);
    }

    // Everything the store already owns, so a row the manifest stopped naming
    // can be *retired*. A removal is a decision about what is there, and the
    // executor guards the preimage it deletes -- which is only meaningful if
    // the file was captured. `sync` declares the same set for the same
    // reason.
    for row in store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
    {
        if let ResourceKey::WholeFile(path) | ResourceKey::SpringTestImport { path, .. } = &row.key
        {
            reads = reads.file(path.clone());
        }
    }

    let request = Request {
        // Complete presence *and* absence for this owner. A capability or an
        // artifact the manifest no longer names is relinquished, which is
        // exactly what a declarative manifest means and what the per-intent
        // loop could never express.
        scope: ReconcileScope::AppManifest,
        declared,
        changes,
    };
    Ok((request, reads))
}

/// The project as every step so far leaves it.
fn projected(
    project: &Project,
    reads: &ReadDeclaration,
    changes: &[DesiredChange],
) -> Result<Project> {
    let (_, mut projection) = capture::projected(project, reads)?;
    for change in changes {
        projection.advance(change)?;
    }
    let mut overlay = BTreeMap::new();
    for (path, entry) in projection.overlay() {
        if let jails_project::projection::ProjectedEntry::File(file) = entry {
            overlay.insert(path.clone(), file.bytes.to_vec());
        }
    }
    Project::projected(project, overlay)
}

/// Everything the next step is allowed to look at, once this one has planned.
///
/// A path a change writes becomes a declared read for the steps after it,
/// because the capture is what they see it through -- and a path nobody
/// declared is a fact the plan may not use.
fn widen(
    reads: ReadDeclaration,
    project: &Project,
    change: &Change,
    desired: &DesiredChange,
) -> Result<ReadDeclaration> {
    let mut reads = reads;
    for artifact in &change.files {
        reads = reads.file(relative_path(project, &artifact.path)?);
    }
    for resource in &desired.resources {
        if let ResourceKey::SpringTestImport { path, .. } = &resource.key {
            reads = reads.file(path.clone());
        }
    }
    Ok(reads)
}
