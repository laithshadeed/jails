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

mod companion;
mod data;
mod evolution;
mod target;

use companion::companion_updates;
use data::{add_data_plan, read_backfill};
use evolution::{
    evolve_existing, projected_column, recipe_package, safe_widening, storage_identity,
};
use jails_protocol::declaration::{FieldType, Optionality};
use jails_protocol::entity::{OneShotId, OneShotSpec, TypeTargetId};
use jails_protocol::identity::SqlName;
use jails_protocol::request::{
    ColumnRenamePolicy, DataEvolution, EvolveFieldRequestV1, FieldEvolution, TypeChangeStrategy,
};
use jails_protocol::resource::{OneShotLifecycle, OneShotState};
use target::recorded_target;

/// Add one component to a generated artifact, and migrate the table for it.
pub fn field(run: &Run, target: &str, component: &str, package: Option<&str>) -> Result<Outcome> {
    add_field_with_syntax(
        run,
        target,
        component,
        package,
        None,
        None,
        &["generate", "field"],
    )
}

/// Compatibility spelling for adding a field with an explicit data plan.
pub fn field_with_data(
    run: &Run,
    target: &str,
    component: &str,
    package: Option<&str>,
    default_literal: Option<&str>,
    backfill_file: Option<&str>,
) -> Result<Outcome> {
    add_field_with_syntax(
        run,
        target,
        component,
        package,
        default_literal,
        backfill_file,
        &["generate", "field"],
    )
}

/// Canonical resource spelling for adding one component.
pub fn add_field(
    run: &Run,
    target: &str,
    component: &str,
    package: Option<&str>,
) -> Result<Outcome> {
    add_field_with_syntax(
        run,
        target,
        component,
        package,
        None,
        None,
        &["resource", "field", "add"],
    )
}

/// Add a field with the explicit data plan required by populated tables.
pub fn add_field_with_data(
    run: &Run,
    target: &str,
    component: &str,
    package: Option<&str>,
    default_literal: Option<&str>,
    backfill_file: Option<&str>,
) -> Result<Outcome> {
    add_field_with_syntax(
        run,
        target,
        component,
        package,
        default_literal,
        backfill_file,
        &["resource", "field", "add"],
    )
}

/// Rename a logical field and its physical column in one explicit cutover.
pub fn rename_field(
    run: &Run,
    target: &str,
    field: &str,
    new_name: &str,
    column: ColumnRenamePolicy,
    package: Option<&str>,
) -> Result<Outcome> {
    match column {
        ColumnRenamePolicy::SingleCutover => {}
        ColumnRenamePolicy::Preserve => {
            return Err("`--column preserve` needs a recorded logical-to-physical column binding.\n       fix: use `--column single-cutover`, or wait until the binding model is available.".into());
        }
        ColumnRenamePolicy::Rolling => {
            return Err("`--column rolling` needs an expand/contract campaign and cannot be reduced to one migration.\n       fix: use `--column single-cutover`, or run the rolling campaign manually.".into());
        }
    }

    let project = run.project();
    let store = observed(project)?;
    let (id, spec) = recorded_target(project, &store, target, package)?;
    let field = Name::parse(field)?;
    let new_name = Name::parse(new_name)?;
    if spec
        .fields()
        .iter()
        .any(|candidate| candidate.name == new_name)
    {
        return Err(format!("`{new_name}` is already a field of `{}`.\n       fix: choose a new logical field name.", id.name).into());
    }
    let mut fields = spec.fields().to_vec();
    let changed = fields
        .iter_mut()
        .find(|candidate| candidate.name == field)
        .ok_or_else(|| unknown_field(&id, &field, spec.fields()))?;
    changed.name = new_name.clone();
    let mut after = spec.clone();
    after.arguments = IntentArguments::Fields(fields);
    for index in &mut after.indexes {
        for indexed in &mut index.columns {
            if indexed.field == field {
                indexed.field = new_name.clone();
            }
        }
    }
    let from = jails_generate::sql::snake_case(field.as_str());
    let to = jails_generate::sql::snake_case(new_name.as_str());
    let body = jails_generate::sql::rename_column(id.name.as_str(), &from, &to);
    evolve_existing(
        run,
        &store,
        target,
        package,
        id,
        after,
        FieldEvolution::Rename {
            field: field.clone(),
            new_name: new_name.clone(),
            column,
        },
        DataEvolution::None,
        body,
        &format!("rename_{from}_to_{to}"),
        vec![target.to_string(), field.to_string(), new_name.to_string()],
        BTreeMap::from([("column".to_string(), vec!["single-cutover".to_string()])]),
    )
}

/// Change a field type when the database mapping proves it is a safe widening.
pub fn change_field_type(
    run: &Run,
    target: &str,
    field: &str,
    to: &str,
    strategy: TypeChangeStrategy,
    package: Option<&str>,
) -> Result<Outcome> {
    if strategy == TypeChangeStrategy::ExpandContract {
        return Err("`--strategy expand-contract` is a multi-release campaign, not one migration.\n       fix: use `--strategy safe` for a proven widening, or run the campaign manually.".into());
    }
    let project = run.project();
    let store = observed(project)?;
    let (id, spec) = recorded_target(project, &store, target, package)?;
    let field = Name::parse(field)?;
    let to = FieldType::parse(to, &Package::parse(project.base())?)?;
    let mut fields = spec.fields().to_vec();
    let changed = fields
        .iter_mut()
        .find(|candidate| candidate.name == field)
        .ok_or_else(|| unknown_field(&id, &field, spec.fields()))?;
    let before_column = projected_column(project, &id, changed, package)?;
    let mut candidate = changed.clone();
    candidate.field_type = to.clone();
    let after_column = projected_column(project, &id, &candidate, package)?;
    if before_column.sql_type == after_column.sql_type {
        return Err(format!(
            "field `{field}` already maps to `{}`.\n       fix: choose a different target type.",
            after_column.sql_type
        )
        .into());
    }
    if !safe_widening(&before_column.sql_type, &after_column.sql_type) {
        return Err(format!("changing `{field}` from `{}` to `{}` is not a proven safe widening.\n       fix: use an explicit expand/contract migration.", before_column.sql_type, after_column.sql_type).into());
    }
    changed.field_type = to.clone();
    let mut after = spec.clone();
    after.arguments = IntentArguments::Fields(fields);
    let body = jails_generate::sql::change_column_type(
        id.name.as_str(),
        &before_column.name,
        &after_column.sql_type,
    );
    evolve_existing(
        run,
        &store,
        target,
        package,
        id,
        after,
        FieldEvolution::ChangeType {
            field: field.clone(),
            to: to.clone(),
            strategy,
        },
        DataEvolution::None,
        body,
        &format!("widen_{}_type", before_column.name),
        vec![target.to_string(), field.to_string()],
        BTreeMap::from([
            ("strategy".to_string(), vec!["safe".to_string()]),
            ("to".to_string(), vec![to.canonical()]),
        ]),
    )
}

/// Make a required field nullable with one forward schema change.
pub fn set_field_nullability(
    run: &Run,
    target: &str,
    field: &str,
    nullable: bool,
    package: Option<&str>,
) -> Result<Outcome> {
    set_field_nullability_with_data(run, target, field, nullable, None, package)
}

/// Change nullability with a reader-owned backfill when making a field required.
pub fn set_field_nullability_with_data(
    run: &Run,
    target: &str,
    field: &str,
    nullable: bool,
    backfill_file: Option<&str>,
    package: Option<&str>,
) -> Result<Outcome> {
    let project = run.project();
    let store = observed(project)?;
    let (id, spec) = recorded_target(project, &store, target, package)?;
    let field = Name::parse(field)?;
    let mut fields = spec.fields().to_vec();
    let changed = fields
        .iter_mut()
        .find(|candidate| candidate.name == field)
        .ok_or_else(|| unknown_field(&id, &field, spec.fields()))?;
    let was_nullable = changed.optionality == Optionality::Nullable;
    if was_nullable == nullable {
        return Err(format!(
            "field `{field}` is already {}.\n       fix: request the opposite nullability.",
            if nullable { "nullable" } else { "required" }
        )
        .into());
    }
    if nullable && backfill_file.is_some() {
        return Err("making a field nullable does not need a backfill.\n       fix: remove `--backfill-file`, or request `--required`.".into());
    }
    if !nullable && backfill_file.is_none() {
        return Err("making a populated column required needs an explicit backfill.\n       fix: pass `--backfill-file <project-path>` so the data update and constraint are one migration.".into());
    }
    if changed.constraints.primary_key {
        return Err(format!(
            "primary-key field `{field}` cannot be nullable.\n       fix: keep the key required, or introduce a separate nullable field."
        )
        .into());
    }
    let column = projected_column(project, &id, changed, package)?;
    changed.optionality = if nullable {
        Optionality::Nullable
    } else {
        Optionality::Required
    };
    let mut after = spec.clone();
    after.arguments = IntentArguments::Fields(fields);
    let (data, body, data_option) = match backfill_file {
        Some(path) => {
            let path = ProjectPath::parse(path)?;
            let sql = read_backfill(project, &path, &column.name)?;
            let body = format!(
                "{}\n\n{}",
                sql.trim_end(),
                jails_generate::sql::set_column_nullable(id.name.as_str(), &column.name, false)
            );
            (
                DataEvolution::ReaderOwnedSql(path.clone()),
                body,
                Some(path.to_string()),
            )
        }
        None => (
            DataEvolution::None,
            jails_generate::sql::set_column_nullable(id.name.as_str(), &column.name, true),
            None,
        ),
    };
    let mut options = BTreeMap::from([(
        if nullable { "nullable" } else { "required" }.to_string(),
        Vec::new(),
    )]);
    if let Some(path) = data_option {
        options.insert("backfill-file".to_string(), vec![path]);
    }
    evolve_existing(
        run,
        &store,
        target,
        package,
        id,
        after,
        FieldEvolution::SetNullability {
            field: field.clone(),
            nullable,
        },
        data,
        body,
        &format!(
            "make_{}_{}",
            column.name,
            if nullable { "nullable" } else { "required" }
        ),
        vec![target.to_string(), field.to_string()],
        options,
    )
}

/// Remove a logical field only when the caller confirms its exact SQL column.
pub fn drop_field(
    run: &Run,
    target: &str,
    field: &str,
    confirmed_column: &str,
    package: Option<&str>,
) -> Result<Outcome> {
    let project = run.project();
    let store = observed(project)?;
    let (id, spec) = recorded_target(project, &store, target, package)?;
    let field = Name::parse(field)?;
    let index = spec
        .fields()
        .iter()
        .position(|candidate| candidate.name == field)
        .ok_or_else(|| unknown_field(&id, &field, spec.fields()))?;
    let removed = &spec.fields()[index];
    if removed.constraints.primary_key {
        return Err(format!(
            "primary-key field `{field}` cannot be dropped by this command.\n       fix: migrate to a replacement key explicitly before removing this field."
        )
        .into());
    }
    if spec
        .indexes
        .iter()
        .any(|spec| spec.columns.iter().any(|column| column.field == field))
    {
        return Err(format!("field `{field}` belongs to a declared index.\n       fix: evolve the index explicitly before dropping the field.").into());
    }
    let expected = jails_generate::sql::snake_case(field.as_str());
    let confirmed_column = SqlName::parse(confirmed_column)?;
    if confirmed_column.as_str() != expected {
        return Err(format!("column confirmation `{}` does not match `{expected}`.\n       fix: pass `--confirm-column {expected}` exactly.", confirmed_column.as_str()).into());
    }
    let mut after = spec.clone();
    let mut fields = spec.fields().to_vec();
    fields.remove(index);
    after.arguments = IntentArguments::Fields(fields);
    let body = jails_generate::sql::drop_column(id.name.as_str(), &expected);
    evolve_existing(
        run,
        &store,
        target,
        package,
        id,
        after,
        FieldEvolution::Drop {
            field: field.clone(),
            confirmed_column,
        },
        DataEvolution::None,
        body,
        &format!("drop_{expected}"),
        vec![target.to_string(), field.to_string()],
        BTreeMap::from([("confirm-column".to_string(), vec![expected])]),
    )
}

fn add_field_with_syntax(
    run: &Run,
    target: &str,
    component: &str,
    package: Option<&str>,
    default_literal: Option<&str>,
    backfill_file: Option<&str>,
    command_path: &[&str],
) -> Result<Outcome> {
    let project = run.project();
    let store = observed(project)?;
    let (id, spec) = recorded_target(project, &store, target, package)?;
    let package = recipe_package(project, &id, package)?;
    let package = package.as_deref();

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
        )
        .into());
    }

    // The whole target, at the spec it becomes. Nothing here compares
    // renders: the store records what jails wrote, so reconciliation decides
    // each derivative on its own.
    let tokens: Vec<String> = existing
        .iter()
        .map(FieldSpec::canonical)
        .chain([added.canonical()])
        .collect();
    let mut change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project,
            &jails_generate::generate::Recipe {
                kind: id.recipe,
                name: id.name.as_str(),
                fields: &tokens,
                indexes: &[],
                strategy_on: None,
                strategy_yields: None,
                // A field overlay never introduces an endpoint: it re-renders
                // a recorded intent with one more component, and the verb --
                // like the references -- is whatever that intent already
                // recorded.
                method: None,
            },
            package,
        )?,
    );
    // Re-render the Java projection, but never carry the scaffold's original
    // create-table migration into field evolution. That file is sealed at
    // publication; the storage half of this command is the new one-shot
    // migration constructed below.
    change.files.retain(|artifact| {
        !artifact
            .path
            .strip_prefix(project.root())
            .is_ok_and(|path| {
                path.to_string_lossy()
                    .replace('\\', "/")
                    .starts_with("src/main/resources/db/migration/")
            })
    });

    let (expected_path, expected_table) = storage_identity(&store, &id, &change, project)?;

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
    let companions = companion_updates(project, &store, &id, after.fields(), package)?;

    // The append-only half, and the reason this is a one-shot at all. It is
    // owned by the field rather than by the target, so removing the target
    // retires the overlay without deleting a migration the database has run.
    let one_shot = OneShotId::Field {
        target: TypeTargetId::Managed(id.clone()),
        field: added.name.clone(),
    };
    let mut migration = DesiredChange::owned_by(ResourceOwner::OneShot(one_shot.clone()));
    // Projected, not re-parsed. `FieldSpec` is the model and `Field` is its
    // rendering half, so this asks the value for the Java facts it implies
    // rather than printing it back to a token for the other parser to read --
    // see `FieldSpec::projected`.
    let parsed = vec![added.projected()?];
    let column = jails_generate::sql::columns(
        &parsed,
        project,
        &project.package_named(jails_spec::spec::layout::DOMAIN, package),
        &jails_generate::generate::lower_first(id.name.as_str()),
    )
    .pop()
    .expect("one field produces one column");
    // The table this column joins, when the resource has one at all. Named by
    // the recorded lifecycle rather than derived from the entity name: a
    // source-only kind has no table, and inventing `tags` for `g record Tag`
    // is what appended `alter table tags` to a project that never created it.
    let table = expected_table
        .as_ref()
        .map(|table| table.as_str().to_string());
    let (data, backfill, data_input) = match &table {
        Some(table) => add_data_plan(
            project,
            &added,
            table,
            &column.name,
            default_literal,
            backfill_file,
        )?,
        None => {
            if let Some(flag) = default_literal
                .map(|_| "--default-literal")
                .or(backfill_file.map(|_| "--backfill-file"))
            {
                return Err(format!(
                    "`{flag}` plans data for a column, and `{} {}` has no table.\n       fix: drop the flag -- a source-only component needs no backfill -- or evolve a scaffold.",
                    label(id.recipe),
                    id.name
                )
                .into());
            }
            (DataEvolution::None, None, None)
        }
    };
    let evolution = EvolveFieldRequestV1 {
        entity: EntityId::Intent(id.clone()),
        expected_path,
        expected_table,
        action: FieldEvolution::Add(added.clone()),
        data: data.clone(),
    };
    let directory = ProjectPath::parse("src/main/resources/db/migration")?;
    let mut migrated = None;
    if let Some(table) = &table
        && project.root().join(directory.as_str()).is_dir()
    {
        let version = jails_generate::generate::next_migration_version(
            &project.root().join(directory.as_str()),
        )?;
        let path = ProjectPath::parse(&format!(
            "{directory}/V{version:03}__add_{}_to_{table}.sql",
            column.name
        ))?;
        let body = match backfill.as_deref() {
            Some(backfill) => jails_generate::sql::add_required_column_with_backfill(
                id.name.as_str(),
                &column,
                backfill,
            )?,
            None => jails_generate::sql::add_column(id.name.as_str(), &column)?,
        };
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
    let mut reads = declaration(project, &change, &desired)?
        .merge(companions.reads)
        .directory(directory);
    if let Some(path) = data_input {
        reads = reads.file(path);
    }
    for path in recorded_migrations(&store, &id) {
        reads = reads.file(path);
    }
    let mut ordered = vec![desired];
    ordered.extend(companions.changes);
    if let Some((path, _)) = &migrated {
        reads = reads.file(path.clone());
        ordered.push(migration);
    }

    let mut declared = companions.entities;
    declared.insert(entity.id.clone(), entity);
    let request = Request {
        scope: ReconcileScope::DirectEntity(EntityId::Intent(id.clone())),
        declared,
        changes: ordered,
    };
    let mut set = request.against(&store)?;
    set.subject = PlannedSubject::EvolveField(Box::new(evolution.clone()));
    let recorded_spec = OneShotSpec::Field {
        target: TypeTargetId::Managed(id.clone()),
        field: added.clone(),
    };
    // Built from the recorded spec rather than rebuilt beside it: two
    // constructions of one value is how a fingerprint comes to describe
    // something the receipt does not.
    let mut options = match package {
        Some(package) => BTreeMap::from([("package".to_string(), vec![package.to_string()])]),
        None => BTreeMap::new(),
    };
    if let Some(value) = default_literal {
        options.insert("default-literal".to_string(), vec![value.to_string()]);
    }
    if let Some(path) = backfill_file {
        options.insert("backfill-file".to_string(), vec![path.to_string()]);
    }
    let asked = Asked::new(
        CanonicalMutationRequest::EvolveField(evolution),
        command_path,
        vec![target.to_string(), component.to_string()],
        options,
        BTreeSet::new(),
    );
    set.ledger_intent.one_shots_after = vec![DesiredOneShotReceipt {
        id: one_shot.clone(),
        spec: recorded_spec,
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
    commit_set(run, set, &reads, &asked)
}

fn unknown_field(id: &IntentId, field: &Name, fields: &[FieldSpec]) -> jails_support::Failure {
    format!(
        "`{field}` is not a field of `{}`.\n       fix: choose one of: {}.",
        id.name,
        fields
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
    .into()
}

fn recorded_migrations(store: &ObservedStore, target: &IntentId) -> BTreeSet<ProjectPath> {
    let lifecycle_paths = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.lifecycles.iter())
        .filter(|lifecycle| lifecycle.entity == EntityId::Intent(target.clone()))
        .flat_map(|lifecycle| lifecycle.migrations.iter().map(|seal| seal.path.clone()));
    let owned_paths = store
        .ledger
        .iter()
        .flat_map(|ledger| ledger.resources.iter())
        .filter(|row| {
            row.owners.iter().any(|owner| match owner {
                ResourceOwner::Entity(EntityId::Intent(owner)) => owner == target,
                ResourceOwner::OneShot(OneShotId::Field {
                    target: TypeTargetId::Managed(owner),
                    ..
                }) => owner == target,
                _ => false,
            })
        })
        .filter_map(|row| match &row.key {
            ResourceKey::WholeFile(path) if row.key.is_migration_history() => Some(path.clone()),
            _ => None,
        });
    lifecycle_paths.chain(owned_paths).collect()
}
