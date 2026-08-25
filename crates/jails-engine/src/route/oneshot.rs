//! `migration` and `cases`: the routes whose subject is not an entity.
//!
//! A one-shot is something that already happened. A migration has been run by
//! the database; a `cases` brief is the reader's file that jails read. Neither
//! is owned, updated or reconciled the way an entity is, so neither goes
//! through [`super::Request`] -- there is nothing to measure against the
//! store. What each states is a receipt plus the file it wrote.

use super::*;

/// Allocate the next forward-only migration, through the protocol.
///
/// §R6.2's `generate_migration` row: *"snapshot allocates next number; lock
/// rechecks directory listing; append-only file/receipt, no destroy."* All
/// three halves are here and none of them is decoration.
///
/// The number comes from a directory listing taken while planning, and that
/// listing is **declared**, so §R4.3 step 2 rechecks it under the lock. Two
/// concurrent runs therefore cannot both allocate `V003`: the second one finds
/// the directory holding something it did not, and refuses as stale rather
/// than writing a second `V003` that Flyway will not apply.
///
/// It is a one-shot rather than an entity because the database has already run
/// it. There is no desired ownership to reconcile, no update, and no destroy —
/// the receipt records that this number was handed out, which is what stops
/// the next run reusing it.
pub fn migration(run: &Run, description: &str) -> Result<Outcome> {
    let project = run.project();
    const DIRECTORY: &str = "src/main/resources/db/migration";
    const BODY: &str = "-- Forward-only migration. Write explicit SQL below.\n";

    let description = jails_generate::generate::sql_name(description)?;
    let listing = ProjectPath::parse(DIRECTORY)?;
    let version =
        jails_generate::generate::next_migration_version(&project.root().join(DIRECTORY))?;
    let path = ProjectPath::parse(&format!("{DIRECTORY}/V{version:03}__{description}.sql"))?;

    let id = OneShotId::Migration { path: path.clone() };
    let owner = ResourceOwner::OneShot(id.clone());
    let key = ResourceKey::WholeFile(path.clone());
    let mut change = DesiredChange::owned_by(owner.clone());
    change.resources.push(DesiredResource::new(
        key.clone(),
        BTreeSet::from([owner]),
        ResourceValue::WholeFile,
    )?);
    change.files.push(DesiredFile {
        path: path.clone(),
        body: DesiredBody::Bytes(BODY.as_bytes().into()),
        mode: None,
        resource: Some(key),
        renderer: None,
    });

    let spec = OneShotSpec::Migration {
        description: description.clone(),
        allocated_version: u64::from(version),
        path: path.clone(),
        body: ObjectId::from_bytes(jails_support::codec::sha256(BODY.as_bytes())),
    };
    provenance::stamp_files(
        &mut change,
        project,
        RendererId::OneShot(OneShotKind::Migration),
        Some(RenderedSubjectContext::OneShot {
            id: id.clone(),
            spec: spec.clone(),
        }),
    )?;
    let observed = observed(project)?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: observed.generation(),
            entities_after: Vec::new(),
            one_shots_after: vec![DesiredOneShotReceipt {
                id: id.clone(),
                spec: spec.clone(),
                // §R1.1: a migration's lifecycle variant is always `Active`.
                // The file it wrote is append-only, so there is no target
                // whose removal could retire it.
                state: OneShotState::Active,
                lifecycle: OneShotLifecycle::Migration,
            }],
            resources_after: change.resources.clone(),
            entities_removed: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::ApplyOneShot {
            id: id.clone(),
            spec: spec.clone(),
        },
    };
    set.validate()?;

    let reads = capture::capability_reads()?.file(path).directory(listing);
    commit_set(
        run,
        set,
        &reads,
        &Asked::plain(
            CanonicalMutationRequest::Generate(CanonicalGenerateRequest::OneShot { id, spec }),
            &["generate", "migration"],
            &[&description],
        ),
    )
}

/// Turn a brief's checklist into a pending test class, through the protocol.
///
/// §R6.2's `generate_cases` row: *"one-shot source-hash receipt; same-source
/// updates reconcile the immutable output path"*. The markdown is an **input**,
/// not something jails owns — it is the reader's file and jails never writes
/// it — so the identity is the source and the content is its hash. That is
/// what makes a re-run an update to a known receipt rather than a second
/// one-shot landing on a file that already exists.
///
/// An external brief is refused by name. §R1.1 gives it a `SourceInputId`
/// keyed by the SHA-256 of its canonical path, with the absolute binding kept
/// only in the runtime commit context — and nothing builds that context yet,
/// so accepting one would record an identity nothing could resolve.
pub fn cases(run: &Run, brief: &str, package: Option<&str>) -> Result<Outcome> {
    let project = run.project();
    let typed = std::path::Path::new(brief);
    let outside = || {
        format!(
            "`{brief}` is outside this project.\n       fix: an external brief needs the runtime \
             commit-context binding plan.md §R1.1 describes, which no command builds yet. Copy \
             the file into the project."
        )
    };
    if typed.is_absolute()
        || typed
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err(jails_support::Failure::Told(outside()));
    }
    let relative = project
        .root()
        .join(typed)
        .strip_prefix(project.root())
        .map_err(|_| outside())?
        .to_path_buf();
    let relative = relative.as_path();
    let source = ProjectPath::parse(
        relative
            .to_str()
            .ok_or_else(|| format!("`{brief}` is not valid UTF-8"))?,
    )?;

    let requested = package;
    let package = project.package_named("", package);
    let (change, markdown) = jails_generate::generate::plan_cases(project, &package, relative)?;
    let output = relative_path(project, &change.files[0].path)?;

    let id = OneShotId::Cases {
        source: SourceInputId::Project(source.clone()),
    };
    let owner = ResourceOwner::OneShot(id.clone());
    let mut desired = desire::contribution(&owner, &change, project)?;

    let spec = OneShotSpec::Cases {
        source: SourceInputId::Project(source.clone()),
        source_sha256: ObjectId::from_bytes(jails_support::codec::sha256(markdown.as_bytes())),
        output,
    };
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::OneShot(OneShotKind::Cases),
        Some(RenderedSubjectContext::OneShot {
            id: id.clone(),
            spec: spec.clone(),
        }),
    )?;
    let observed = observed(project)?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: observed.generation(),
            entities_after: Vec::new(),
            one_shots_after: vec![DesiredOneShotReceipt {
                id: id.clone(),
                spec: spec.clone(),
                state: OneShotState::Active,
                lifecycle: OneShotLifecycle::Cases,
            }],
            resources_after: desired.resources.clone(),
            entities_removed: Vec::new(),
        },
        ordered: vec![desired],
        subject: PlannedSubject::ApplyOneShot {
            id: id.clone(),
            spec: spec.clone(),
        },
    };
    set.validate()?;

    // The brief is declared even though nothing writes it: its bytes are what
    // the receipt hashes, so an edit between the plan and the commit would
    // make the receipt describe a file that no longer says that.
    let reads = capture::capability_reads()?
        .file(source)
        .file(relative_path(project, &change.files[0].path)?);
    commit_set(
        run,
        set,
        &reads,
        &Asked::new(
            CanonicalMutationRequest::Generate(CanonicalGenerateRequest::OneShot { id, spec }),
            &["generate", "cases"],
            vec![brief.to_string()],
            match requested {
                Some(package) => {
                    BTreeMap::from([("package".to_string(), vec![package.to_string()])])
                }
                None => BTreeMap::new(),
            },
            BTreeSet::new(),
        ),
    )
}
