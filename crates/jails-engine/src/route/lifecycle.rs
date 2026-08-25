//! Mutating resource-lifecycle routes built from durable identity and model.

use super::*;
use jails_protocol::lifecycle::ResourceLifecycleV1;
use jails_protocol::lifecycle::ResourceState;
use jails_protocol::request::{
    DatasourceRef, RepairResourceRequestV1, RepairStrategy, ReviveResourceRequestV1,
};

/// Regenerate projections for a preserved table without publishing another
/// create migration or inventing a new entity identity.
pub fn revive(run: &Run, selector: &str, table: &str) -> Result<Outcome> {
    let project = run.project();
    let store = observed(project)?;
    let lifecycle = selected_lifecycle(&store, selector)?.clone();
    let EntityId::Intent(id) = &lifecycle.entity else {
        return Err("only a generated intent can be revived.\n       fix: select a scaffold lifecycle identity".into());
    };
    let expected_table = jails_protocol::identity::SqlName::parse(table)?;
    let recorded_table = lifecycle.table.as_ref().ok_or_else(|| {
        format!(
            "resource `{selector}` has no durable table binding.\n       fix: inspect `jails \
             resource status {selector}` and repair its identity first"
        )
    })?;
    if expected_table != recorded_table.table {
        return Err(format!(
            "table `{table}` is not the preserved table `{}`.\n       fix: pass `--table {}` exactly",
            recorded_table.table.as_str(),
            recorded_table.table.as_str()
        )
        .into());
    }
    let EntitySpec::Intent(spec) = &lifecycle.last_spec else {
        return Err(format!(
            "resource `{selector}` has no generated intent model to revive.\n       fix: inspect \
             `jails resource status {selector}` and repair the recorded authority"
        )
        .into());
    };

    let fields = spec.arguments.canonical();
    let indexes = spec
        .indexes
        .iter()
        .map(|index| index.canonical())
        .collect::<Vec<_>>();
    let on = spec.on.as_ref().map(JavaType::qualified);
    let yields = spec.yields.as_ref().map(JavaType::qualified);
    let package = relative_package(project, id.package.as_str())?;
    let mut change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project,
            &Recipe {
                kind: id.recipe,
                name: id.name.as_str(),
                fields: &fields,
                indexes: &indexes,
                strategy_on: on.as_deref(),
                strategy_yields: yields.as_deref(),
                method: spec.method,
            },
            package.as_deref(),
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

    let owner = ResourceOwner::Entity(lifecycle.entity.clone());
    let mut desired = desire::contribution(&owner, &change, project)?;
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Recipe(id.recipe),
        Some(RenderedSubjectContext::Entity {
            id: lifecycle.entity.clone(),
            spec: lifecycle.last_spec.clone(),
        }),
    )?;
    let entity = DesiredEntity {
        id: lifecycle.entity.clone(),
        spec: lifecycle.last_spec.clone(),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let mut reads = declaration(project, &change, &desired)?;
    for seal in &lifecycle.migrations {
        reads = reads.file(seal.path.clone());
    }
    let request = Request {
        scope: ReconcileScope::DirectEntity(lifecycle.entity.clone()),
        declared: BTreeMap::from([(entity.id.clone(), entity)]),
        changes: vec![desired],
    };
    let canonical = ReviveResourceRequestV1 {
        entity: lifecycle.entity.clone(),
        expected_table,
    };
    let asked = Asked::new(
        CanonicalMutationRequest::ReviveResource(canonical.clone()),
        &["resource", "revive"],
        vec![selector.to_string()],
        BTreeMap::from([("table".to_string(), vec![table.to_string()])]),
        BTreeSet::new(),
    );
    commit_subject(
        run,
        request,
        &reads,
        &asked,
        PlannedSubject::ReviveResource(Box::new(canonical)),
    )
}

/// Restore content-addressed migration history and reconcile owned projections.
pub fn repair(run: &Run, selector: &str, datasource: Option<&str>) -> Result<Outcome> {
    if let Some(datasource) = datasource {
        return Err(format!(
            "live repair evidence for datasource `{datasource}` is not available yet.\n       fix: \
             omit `--datasource` to repair from sealed local authority, or inspect it with \
             `jails resource status {selector} --datasource {datasource}`."
        )
        .into());
    }
    let project = run.project();
    let store = observed(project)?;
    let lifecycle = selected_lifecycle(&store, selector)?.clone();
    if !matches!(lifecycle.state, ResourceState::Active) {
        return Err(format!(
            "resource `{selector}` is retired, so repair cannot recreate its projections.\n       \
             fix: use `jails resource revive {selector} --table <recorded-table>` first."
        )
        .into());
    }
    let EntityId::Intent(id) = &lifecycle.entity else {
        return Err("only a generated intent can be repaired.\n       fix: select a scaffold lifecycle identity".into());
    };
    let EntitySpec::Intent(spec) = &lifecycle.last_spec else {
        return Err(format!(
            "resource `{selector}` has no generated intent model to repair.\n       fix: restore \
             the recorded authority before repairing files."
        )
        .into());
    };

    let fields = spec.arguments.canonical();
    let indexes = spec
        .indexes
        .iter()
        .map(|index| index.canonical())
        .collect::<Vec<_>>();
    let on = spec.on.as_ref().map(JavaType::qualified);
    let yields = spec.yields.as_ref().map(JavaType::qualified);
    let package = relative_package(project, id.package.as_str())?;
    let mut change = with_test_support(
        project,
        jails_generate::generate::plan_recipe(
            project,
            &Recipe {
                kind: id.recipe,
                name: id.name.as_str(),
                fields: &fields,
                indexes: &indexes,
                strategy_on: on.as_deref(),
                strategy_yields: yields.as_deref(),
                method: spec.method,
            },
            package.as_deref(),
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
    let owner = ResourceOwner::Entity(lifecycle.entity.clone());
    let mut desired = desire::contribution(&owner, &change, project)?;
    provenance::stamp_files(
        &mut desired,
        project,
        RendererId::Recipe(id.recipe),
        Some(RenderedSubjectContext::Entity {
            id: lifecycle.entity.clone(),
            spec: lifecycle.last_spec.clone(),
        }),
    )?;

    let object_store = jails_commit::store::Store::at(project.root()).objects();
    let mut migrations = DesiredChange::owned_by(owner.clone());
    for seal in &lifecycle.migrations {
        let bytes = jails_commit::store::read_object(&object_store, &seal.content_digest).map_err(
            |_| {
                format!(
                    "sealed bytes for `{}` are missing or corrupt.\n       fix: restore object `{}` \
                     under `.jails/objects` from backup, then retry.",
                    seal.path, seal.content_digest
                )
            },
        )?;
        let key = ResourceKey::WholeFile(seal.path.clone());
        migrations.resources.push(DesiredResource::new(
            key.clone(),
            BTreeSet::from([owner.clone()]),
            ResourceValue::WholeFile,
        )?);
        migrations.files.push(DesiredFile {
            path: seal.path.clone(),
            body: DesiredBody::Bytes(bytes.into()),
            mode: None,
            resource: Some(key),
            renderer: None,
        });
    }

    let entity = DesiredEntity {
        id: lifecycle.entity.clone(),
        spec: lifecycle.last_spec.clone(),
        owners: BTreeSet::from([OwnerId::DirectCli]),
    };
    let mut reads = declaration(project, &change, &desired)?;
    for seal in &lifecycle.migrations {
        reads = reads.file(seal.path.clone());
    }
    let request = Request {
        scope: ReconcileScope::DirectEntity(lifecycle.entity.clone()),
        declared: BTreeMap::from([(entity.id.clone(), entity)]),
        changes: vec![desired, migrations],
    };
    let canonical = RepairResourceRequestV1 {
        entity: lifecycle.entity.clone(),
        expected_path: lifecycle.expected_path.clone(),
        strategy: RepairStrategy::RollForward,
        datasource: datasource.map(DatasourceRef::parse).transpose()?,
    };
    let mut options = BTreeMap::from([("strategy".to_string(), vec!["roll-forward".to_string()])]);
    if let Some(datasource) = datasource {
        options.insert("datasource".to_string(), vec![datasource.to_string()]);
    }
    let asked = Asked::new(
        CanonicalMutationRequest::RepairResource(canonical.clone()),
        &["resource", "repair"],
        vec![selector.to_string()],
        options,
        BTreeSet::new(),
    );
    commit_subject(
        run,
        request,
        &reads,
        &asked,
        PlannedSubject::RepairResource(Box::new(canonical)),
    )
}

fn selected_lifecycle<'a>(
    store: &'a ObservedStore,
    selector: &str,
) -> Result<&'a ResourceLifecycleV1> {
    let matches = store
        .lifecycles()
        .iter()
        .filter(|lifecycle| {
            lifecycle.expected_path.qualified() == selector
                || matches!(&lifecycle.entity, EntityId::Intent(id) if id.name.as_str().eq_ignore_ascii_case(selector))
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [lifecycle] => Ok(lifecycle),
        [] => Err(format!(
            "no resource lifecycle matches `{selector}`.\n       fix: run `jails resource status \
             {selector}` to inspect recorded identities"
        )
        .into()),
        _ => Err(format!(
            "`{selector}` matches {} resource lifecycles.\n       fix: pass the fully qualified Java type",
            matches.len()
        )
        .into()),
    }
}

fn relative_package(project: &Project, recorded: &str) -> Result<Option<String>> {
    let base = project.base();
    if recorded == base {
        return Ok(None);
    }
    let prefix = format!("{base}.");
    recorded
        .strip_prefix(&prefix)
        .map(|relative| Some(relative.to_string()))
        .ok_or_else(|| {
            format!(
                "recorded package `{recorded}` is outside project base `{base}`.\n       fix: repair \
                 the lifecycle identity before reviving it"
            )
            .into()
        })
}
