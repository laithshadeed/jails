use super::*;

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
    let fields: Vec<String> = after.fields().iter().map(FieldSpec::canonical).collect();
    let indexes: Vec<String> = after
        .indexes
        .iter()
        .map(|index| index.canonical())
        .collect();
    let on = after.on.as_ref().map(JavaType::qualified);
    let yields = after.yields.as_ref().map(JavaType::qualified);
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
                method: after.method,
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
    let companions = companion_updates(project, store, &id, after.fields(), package)?;

    let directory = ProjectPath::parse("src/main/resources/db/migration")?;
    if !project.root().join(directory.as_str()).is_dir() {
        return Err("field evolution requires `src/main/resources/db/migration`.\n       fix: add Flyway or scaffold the resource before evolving it.".into());
    }
    let version =
        jails_generate::generate::next_migration_version(&project.root().join(directory.as_str()))?;
    let path = ProjectPath::parse(&format!("{directory}/V{version:03}__{migration_slug}.sql"))?;
    let key = ResourceKey::WholeFile(path.clone());
    let mut migration = DesiredChange::owned_by(owner.clone());
    migration.resources.push(DesiredResource::new(
        key.clone(),
        BTreeSet::from([owner]),
        ResourceValue::WholeFile,
    )?);
    migration.files.push(DesiredFile {
        path: path.clone(),
        body: DesiredBody::Bytes(migration_body.as_bytes().into()),
        mode: None,
        resource: Some(key),
        renderer: None,
    });

    let entity = DesiredEntity {
        id: EntityId::Intent(id.clone()),
        spec: EntitySpec::Intent(after),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let mut reads = declaration(project, &change, &desired)?
        .merge(companions.reads)
        .directory(directory);
    for recorded in recorded_migrations(store, &id) {
        reads = reads.file(recorded);
    }
    if let DataEvolution::ReaderOwnedSql(path) = &data {
        reads = reads.file(path.clone());
    }
    reads = reads.file(path);
    let mut declared = companions.entities;
    declared.insert(entity.id.clone(), entity);
    let mut changes = vec![desired];
    changes.extend(companions.changes);
    changes.push(migration);
    let request = Request {
        scope: ReconcileScope::DirectEntity(EntityId::Intent(id.clone())),
        declared,
        changes,
    };
    let expected_path = JavaType::new(id.package.clone(), id.name.clone());
    let evolution = EvolveFieldRequestV1 {
        entity: EntityId::Intent(id.clone()),
        expected_path,
        expected_table: SqlName::parse(&jails_generate::sql::table_name(id.name.as_str()))?,
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
