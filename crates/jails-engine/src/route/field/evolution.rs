use super::*;

/// Where a field evolution's Java lives, and which table -- if any -- it
/// migrates.
///
/// Two answers, and the difference between them is the whole point. A
/// `scaffold` owns a table, so its identity has to come from the *recorded*
/// lifecycle: deriving it from the entity name instead is how a rename came to
/// be planned against a path the ledger did not hold. Every other kind is
/// source-only -- a `record` is a Java record and nothing else -- so it has no
/// column to alter, and saying so with `None` is what keeps `alter table tags`
/// out of a project that has never had a `tags` table.
pub(super) fn storage_identity(
    store: &ObservedStore,
    id: &IntentId,
    change: &jails_project::model::Change,
    project: &Project,
) -> Result<(JavaType, Option<SqlName>)> {
    if id.recipe != ArtifactKind::Scaffold {
        return Ok((source_only_path(project, id, change)?, None));
    }
    let entity = EntityId::Intent(id.clone());
    let lifecycle = store
        .lifecycles()
        .iter()
        .find(|lifecycle| lifecycle.entity == entity)
        .ok_or_else(|| {
            format!(
                "scaffold `{}` has no recorded storage lifecycle.\n       fix: inspect `jails resource status {}` and repair or revive it before evolving fields.",
                id.name, id.name
            )
        })?;
    let table = lifecycle.table.as_ref().ok_or_else(|| {
        format!(
            "scaffold `{}` has no recorded table binding.\n       fix: inspect `jails resource status {}` and adopt its exact table before evolving fields.",
            id.name, id.name
        )
    })?;
    Ok((lifecycle.expected_path.clone(), Some(table.table.clone())))
}

/// The generated Java type of a source-only resource, read off the plan that
/// places it.
///
/// A scaffold reads this from its recorded lifecycle. A source-only kind has
/// no lifecycle until its first evolution bootstraps one, so the path is taken
/// from the artifact the generator just planned -- deriving it rather than
/// keeping a second kind-to-layer table, for the same reason `destroy` has no
/// path table: a second copy is one that drifts.
fn source_only_path(
    project: &Project,
    id: &IntentId,
    change: &jails_project::model::Change,
) -> Result<JavaType> {
    let file = format!("{}.java", id.name);
    let sources = project.root().join("src/main/java");
    let placed = change
        .files
        .iter()
        .filter_map(|artifact| artifact.path.strip_prefix(&sources).ok())
        .find(|path| path.file_name().is_some_and(|name| name == file.as_str()))
        .ok_or_else(|| {
            format!(
                "`{} {}` plans no `{file}` under `src/main/java`.\n       fix: regenerate the resource before evolving its fields.",
                label(id.recipe),
                id.name
            )
        })?;
    let package = placed
        .parent()
        .map(|parent| parent.to_string_lossy().replace(['/', '\\'], "."))
        .unwrap_or_default();
    Ok(JavaType::new(Package::parse(&package)?, id.name.clone()))
}

pub(super) fn recipe_package(
    project: &Project,
    id: &IntentId,
    requested: Option<&str>,
) -> Result<Option<String>> {
    if let Some(requested) = requested {
        return Ok(Some(requested.to_string()));
    }
    if id.package.as_str() == project.base() {
        return Ok(None);
    }
    let prefix = format!("{}.", project.base());
    id.package
        .as_str()
        .strip_prefix(&prefix)
        .map(|relative| Some(relative.to_string()))
        .ok_or_else(|| {
            format!(
                "recorded package `{}` is outside project base `{}`.\n       fix: repair the entity identity before evolving its fields.",
                id.package,
                project.base()
            )
            .into()
        })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn evolve_existing(
    run: &Run,
    store: &ObservedStore,
    _target: &str,
    package: Option<&str>,
    id: IntentId,
    after: IntentSpec,
    action: FieldEvolution,
    data: DataEvolution,
    migration_body: String,
    migration_slug: &str,
    positionals: Vec<String>,
    mut options: BTreeMap<String, Vec<String>>,
) -> Result<Outcome> {
    let project = run.project();
    let package = recipe_package(project, &id, package)?;
    let package = package.as_deref();
    let fields: Vec<String> = after.fields().iter().map(FieldSpec::canonical).collect();
    // Columns, not components: the generator writes DDL. See
    // `request::as_column_names` for what passing `canonical()` here cost.
    let indexes: Vec<String> = after
        .indexes
        .iter()
        .map(|index| super::request::as_column_names(index, after.fields()))
        .collect();
    let on = after.on.as_ref().map(JavaType::qualified);
    let yields = after.yields.as_ref().map(JavaType::qualified);
    let via = after.via.as_ref().map(JavaType::qualified);
    let order_by = ordering_token(&after);
    let limit = after.limit;
    let on_conflict = after.on_conflict.as_ref().map(ToString::to_string);
    let path = after.path.as_ref().map(ToString::to_string);
    let mut change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project,
            &jails_generate::generate::Recipe {
                kind: id.recipe,
                name: id.name.as_str(),
                fields: &fields,
                indexes: &indexes,
                strategy_on: on.as_deref(),
                strategy_yields: yields.as_deref(),
                via: via.as_deref(),
                order_by: order_by.as_deref(),
                limit,
                on_conflict: on_conflict.as_deref(),
                path: path.as_deref(),
                method: after.method,
                consumes: after.consumes,
            },
            package,
        )?,
    );
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

    let (expected_path, expected_table) = storage_identity(store, &id, &change, project)?;

    let owner = ResourceOwner::Entity(EntityId::Intent(id.clone()));
    let mut desired = desire::contribution(&owner, &change, project)?;
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Recipe(id.recipe),
        Some(RenderedSubjectContext::Entity {
            id: EntityId::Intent(id.clone()),
            spec: EntitySpec::Intent(after.clone()),
        }),
    )?;
    // The primary's own declaration is what the dependants must plan against,
    // so it is computed before them and handed over as the projection base.
    let primary_reads = declaration(project, &change, &desired)?;
    let companions = companion_updates(
        project,
        store,
        &id,
        after.fields(),
        package,
        &desired,
        &primary_reads,
    )?;

    // The storage half, and it exists only when there is storage. A
    // source-only resource -- a `record`, a `value` -- has no column to alter,
    // so appending `alter table <plural>` for it would publish a migration
    // naming a table the project has never created, which is unappliable
    // everywhere and reported nowhere.
    //
    // An *empty* body is the second case with no migration, and it is a
    // different fact: `--column preserve` moves the Java name and leaves the
    // column, so there is storage and nothing for it to run. Writing the file
    // anyway would put a checksum in Flyway's history asserting a change that
    // never happened. plan.md P3.2.
    let mut migration = None;
    if expected_table.is_some() && !migration_body.is_empty() {
        let directory = ProjectPath::parse("src/main/resources/db/migration")?;
        if !project.root().join(directory.as_str()).is_dir() {
            return Err("field evolution requires `src/main/resources/db/migration`.\n       fix: add Flyway or scaffold the resource before evolving it.".into());
        }
        let version = jails_generate::generate::next_migration_version(
            &project.root().join(directory.as_str()),
        )?;
        let path = ProjectPath::parse(&format!("{directory}/V{version:03}__{migration_slug}.sql"))?;
        let key = ResourceKey::WholeFile(path.clone());
        let mut change = DesiredChange::owned_by(owner.clone());
        change.resources.push(DesiredResource::new(
            key.clone(),
            BTreeSet::from([owner.clone()]),
            ResourceValue::WholeFile,
        )?);
        change.files.push(DesiredFile {
            path: path.clone(),
            body: DesiredBody::Bytes(migration_body.as_bytes().into()),
            mode: None,
            resource: Some(key),
            renderer: None,
        });
        migration = Some((directory, path, change));
    }

    let entity = DesiredEntity {
        id: EntityId::Intent(id.clone()),
        spec: EntitySpec::Intent(after),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let mut reads = primary_reads.merge(companions.reads);
    if let Some((directory, path, _)) = &migration {
        reads = reads.directory(directory.clone()).file(path.clone());
    }
    // Observed whether or not this command appends one. A sealed migration is
    // read by the coherence check on every field evolution, and a
    // `--column preserve` rename writes none -- so gating this on `migration`
    // made the one path with no new migration plan against files it had not
    // observed. plan.md P3.2.
    for recorded in recorded_migrations(store, &id) {
        reads = reads.file(recorded);
    }
    if let DataEvolution::ReaderOwnedSql(path) = &data {
        reads = reads.file(path.clone());
    }
    let mut declared = companions.entities;
    declared.insert(entity.id.clone(), entity);
    let mut changes = vec![desired];
    changes.extend(companions.changes);
    changes.extend(migration.map(|(_, _, change)| change));
    let request = Request {
        scope: ReconcileScope::DirectEntity(EntityId::Intent(id.clone())),
        declared,
        changes,
    };
    let evolution = EvolveFieldRequestV1 {
        entity: EntityId::Intent(id.clone()),
        expected_path,
        expected_table,
        action,
        data,
    };
    let mut set = request.against(store)?;
    set.subject = PlannedSubject::EvolveField(Box::new(evolution.clone()));
    if let Some(package) = package {
        options.insert("package".to_string(), vec![package.to_string()]);
    }
    let asked = Asked::new(
        CanonicalMutationRequest::EvolveField(evolution),
        &["resource", "field", action_name(&set.subject)],
        positionals,
        options,
        BTreeSet::new(),
    );
    set.validate()?;
    commit_set(run, set, &reads, &asked)
}

fn action_name(subject: &PlannedSubject) -> &'static str {
    match subject {
        PlannedSubject::EvolveField(request) => match request.action {
            FieldEvolution::Add(_) => "add",
            FieldEvolution::Rename { .. } => "rename",
            FieldEvolution::ChangeType { .. } => "type",
            FieldEvolution::SetNullability { .. } => "nullability",
            FieldEvolution::Drop { .. } => "drop",
            FieldEvolution::AddIndex(_) => "index",
        },
        _ => unreachable!("field evolution constructs a field subject"),
    }
}

pub(super) fn projected_column(
    project: &Project,
    id: &IntentId,
    field: &FieldSpec,
    package: Option<&str>,
) -> Result<jails_generate::sql::Column> {
    Ok(jails_generate::sql::columns(
        &[field.projected()?],
        project,
        &project.package_named(jails_spec::spec::layout::DOMAIN, package),
        &jails_generate::generate::lower_first(id.name.as_str()),
    )
    .pop()
    .expect("one field produces one column"))
}

pub(super) fn safe_widening(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("integer", "bigint")
            | ("integer", "numeric")
            | ("integer", "double precision")
            | ("bigint", "numeric")
            | ("bigint", "double precision")
    )
}
