//! `g field`: one component added to an artifact that already exists.
//!
//! §R6.2's `generate_field` row. What makes it a one-shot rather than an
//! ordinary update is the partition: the *derivative* files -- the record, its
//! test, the repository adapter, the DTOs -- are target-coupled and die with
//! the target, while the migration it writes is **append-only**, because the
//! database has already run it and removing the record does not un-add the
//! column.
//!
//! ## What the recorded base bought
//!
//! V1 renders the target twice -- once at the old field list, once at the new
//! -- and compares the *old* render against what is on disk to decide whether
//! the reader has edited a derivative. That works and is fragile in a
//! particular way: the comparison is against bytes a *newer* binary rendered
//! from an older spec, so a template change between the two runs reads as a
//! user edit and the file is skipped with a note.
//!
//! Here the target is simply re-desired at the new spec and §R5.3 answers the
//! question from the bytes jails actually wrote. A reader's edit keeps their
//! bytes; an untouched derivative is replaced; a derivative that already holds
//! the new bytes advances its base and writes nothing.

use super::*;

use jails_protocol::entity::{OneShotId, OneShotSpec, TypeTargetId};
use jails_protocol::resource::{OneShotLifecycle, OneShotState};

/// Add one component to a generated artifact, and migrate the table for it.
pub fn field(
    project: &Project,
    target: &str,
    component: &str,
    package: Option<&str>,
) -> Result<CommitResult> {
    let store = observed(project)?;
    let (id, spec) = recorded_target(project, &store, target, package)?;

    let base = Package::parse(project.base())?;
    let added = FieldSpec::parse(component, &base)?;
    let existing = spec.fields();
    if existing.iter().any(|field| field.name == added.name) {
        return Err(format!(
            "`{} {}` already has a `{}` component.\n       fix: choose another name. Removing or \
             changing a component is a data migration, and jails does not write one it cannot \
             check against the rows that are there.",
            label(id.recipe),
            id.name,
            added.name
        ));
    }

    // The whole target, at the spec it becomes. Nothing here compares
    // renders: the store records what jails wrote, so reconciliation decides
    // each derivative on its own.
    let tokens: Vec<String> = existing
        .iter()
        .map(FieldSpec::canonical)
        .chain([added.canonical()])
        .collect();
    let change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project,
            id.recipe,
            id.name.as_str(),
            &tokens,
            package,
            &[],
            None,
            None,
        )?,
    );

    let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
    let mut desired = desire::contribution(&owner, &change, project)?;
    let after = IntentSpec {
        arguments: jails_protocol::declaration::IntentArguments::Fields(
            existing.iter().cloned().chain([added.clone()]).collect(),
        ),
        ..spec.clone()
    };
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Recipe(id.recipe),
        Some(RenderedSubjectContext::Entity {
            id: EntityId::Intent(id.clone()),
            spec: EntitySpec::Intent(after.clone()),
        }),
    )?;

    // The append-only half, and the reason this is a one-shot at all. It is
    // owned by the field rather than by the target, so removing the target
    // retires the overlay without deleting a migration the database has run.
    let one_shot = OneShotId::Field {
        target: TypeTargetId::Managed(id.clone()),
        field: added.name.clone(),
    };
    let mut migration = DesiredChange::owned_by(ResourceOwner::OneShot(one_shot.clone()));
    // Back through the recipe layer's own parser, from the canonical spelling
    // rather than by translating the value: two parsers for one syntax is how
    // the column and the record component come to disagree.
    let parsed = jails_generate::generate::parse_fields(&[added.canonical()])?;
    let column = jails_generate::sql::columns(
        &parsed,
        project,
        &project.package_named(jails_spec::spec::layout::DOMAIN, package),
        &jails_generate::generate::lower_first(id.name.as_str()),
    )
    .pop()
    .expect("one field produces one column");
    let table = jails_generate::sql::table_name(id.name.as_str());
    let directory = ProjectPath::parse("src/main/resources/db/migration")?;
    let mut migrated = None;
    if project.root().join(directory.as_str()).is_dir() {
        let version = jails_generate::generate::next_migration_version(
            &project.root().join(directory.as_str()),
        )?;
        let path = ProjectPath::parse(&format!(
            "{directory}/V{version:03}__add_{}_to_{table}.sql",
            column.name
        ))?;
        let body = jails_generate::sql::add_column(id.name.as_str(), &column)?;
        let key = ResourceKey::WholeFile(path.clone());
        migration.resources.push(DesiredResource::new(
            key.clone(),
            BTreeSet::from([ResourceOwner::OneShot(one_shot.clone())]),
            ResourceValue::WholeFile,
        )?);
        migration.files.push(DesiredFile {
            path: path.clone(),
            body: DesiredBody::Bytes(body.as_bytes().into()),
            mode: None,
            resource: Some(key.clone()),
            renderer: None,
        });
        migrated = Some((path, key));
    }

    let entity = DesiredEntity {
        id: EntityId::Intent(id.clone()),
        spec: EntitySpec::Intent(after),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let mut reads = declaration(project, &change, &desired)?.directory(directory);
    let mut ordered = vec![desired];
    if let Some((path, _)) = &migrated {
        reads = reads.file(path.clone());
        ordered.push(migration);
    }

    let request = Request {
        scope: ReconcileScope::DirectEntity(EntityId::Intent(id.clone())),
        declared: BTreeMap::from([(entity.id.clone(), entity)]),
        changes: ordered,
    };
    let mut set = request.against(&store)?;
    set.ledger_intent.one_shots_after = vec![DesiredOneShotReceipt {
        id: one_shot.clone(),
        spec: OneShotSpec::Field {
            target: TypeTargetId::Managed(id),
            field: added,
        },
        state: OneShotState::Active,
        lifecycle: OneShotLifecycle::Field {
            // The derivatives are the target's own resources, so they are
            // already partitioned by ownership; what this records is the half
            // that must survive the target's removal.
            target_coupled: BTreeSet::new(),
            append_only: migrated
                .into_iter()
                .map(|(_, key)| key)
                .collect::<BTreeSet<_>>(),
        },
    }];
    set.validate()?;
    commit_set(project, set, &reads, "jails generate field")
}

/// The artifact this field is being added to, as the store records it.
///
/// Recorded rather than read off disk, because the spec is what the next
/// render is computed from and a record's Java cannot say what its components
/// were *declared* as: `@pk`, `@unique` and `@index` change the DDL and
/// nothing about the type. Reading them back would produce a table missing
/// the key somebody believed they had asked for.
fn recorded_target(
    project: &Project,
    store: &ObservedStore,
    target: &str,
    package: Option<&str>,
) -> Result<(IntentId, IntentSpec)> {
    let name = Name::parse(&jails_spec::spec::field::capitalize(target))?;
    let package = Package::parse(&project.package_named("", package))?;
    let found = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.applied.iter())
        .find_map(|row| match (&row.id, &row.version.spec) {
            (EntityId::Intent(id), EntitySpec::Intent(spec))
                if id.name == name && id.package == package =>
            {
                Some((id.clone(), spec.clone()))
            }
            _ => None,
        });
    found.ok_or_else(|| {
        format!(
            "no `{name}` is recorded in this project.\n       fix: `jails g scaffold {name} \
             ...` or `jails g record {name} ...` first. Adding a component to something the \
             store never recorded would mean guessing what its other components were declared \
             as, and a declaration is not readable from the Java it produced."
        )
    })
}
