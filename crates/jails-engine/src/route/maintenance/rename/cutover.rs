use super::*;

pub(super) struct CutoverPlan {
    pub(super) current: jails_protocol::identity::SqlName,
    pub(super) target: jails_protocol::identity::SqlName,
    pub(super) migration: ProjectPath,
    pub(super) artifact: jails_project::model::Artifact,
    pub(super) sql_sources: Vec<ProjectPath>,
}

pub(super) fn prepare_cutover(
    project: &Project,
    store: &ObservedStore,
    resource_request: Option<&(String, jails_protocol::request::RenameResourceRequestV1)>,
) -> Result<Option<CutoverPlan>> {
    let Some((_, request)) = resource_request else {
        return Ok(None);
    };
    if request.strategy != jails_protocol::request::RenameStrategy::SingleCutover {
        return Ok(None);
    }
    let lifecycle = store
        .lifecycles()
        .iter()
        .find(|lifecycle| lifecycle.entity == request.entity)
        .ok_or("single-cutover target has no adopted lifecycle.\n       fix: adopt its lifecycle before retrying")?;
    let current = lifecycle
        .table
        .as_ref()
        .ok_or("single-cutover target has no explicit current table binding.\n       fix: adopt an explicit binding before retrying")?
        .table
        .clone();
    let target = request
        .target_table
        .clone()
        .ok_or("single-cutover request has no resolved target table.\n       fix: prepare the rename again with an explicit target")?;
    if store.lifecycles().iter().any(|held| {
        held.entity != request.entity
            && held
                .table
                .as_ref()
                .is_some_and(|binding| binding.table == target)
    }) {
        return Err(format!(
            "target table `{}` is already bound to another managed resource.\n       fix: choose an unused table name",
            target.as_str()
        )
        .into());
    }
    let directory = project.root().join("src/main/resources/db/migration");
    if !directory.is_dir() {
        return Err("single-cutover requires `src/main/resources/db/migration`.\n       fix: add Flyway before renaming physical storage".into());
    }
    let generated =
        jails_generate::generate::rename_table_change(project, current.as_str(), target.as_str())?;
    let [artifact] = generated.files.as_slice() else {
        return Err("table cutover generator did not produce exactly one migration.\n       fix: restore the built-in migration renderer and retry".into());
    };
    let migration = crate::route::relative_path(project, &artifact.path)?;
    let sql_sources = sql_sources(project)?;
    Ok(Some(CutoverPlan {
        current,
        target,
        migration,
        artifact: artifact.clone(),
        sql_sources,
    }))
}

fn sql_sources(project: &Project) -> Result<Vec<ProjectPath>> {
    let root = project.root().join("src/main/resources");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut pending = vec![root];
    let mut sources = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory)
            .map_err(|error| format!("failed to read {}: {error}", directory.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                format!(
                    "failed to read an entry in {}: {error}",
                    directory.display()
                )
            })?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "sql") {
                sources.push(crate::route::relative_path(project, &path)?);
            }
        }
    }
    sources.sort();
    Ok(sources)
}

pub(super) fn validate_cutover_sql(
    snapshot: &jails_protocol::snapshot::ProjectSnapshot,
    cutover: &CutoverPlan,
) -> Result<()> {
    let mut manual = Vec::new();
    for path in &cutover.sql_sources {
        let jails_protocol::snapshot::Captured::Present(file) = snapshot.read(path)? else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&file.bytes) else {
            if !ResourceKey::WholeFile(path.clone()).is_migration_history() {
                manual.push(path.clone());
            }
            continue;
        };
        if ResourceKey::WholeFile(path.clone()).is_migration_history() {
            if declares_table(text, cutover.target.as_str()) {
                return Err(format!(
                    "target-table-collision: migration history already declares `{}` in `{path}`.\n       fix: choose an unused target table",
                    cutover.target.as_str()
                )
                .into());
            }
        } else if jails_java::identifier::bounded_mentions(text, cutover.current.as_str()) > 0 {
            manual.push(path.clone());
        }
    }
    if manual.is_empty() {
        return Ok(());
    }
    let paths = manual
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n         ");
    Err(format!(
        "manual-edit-required: reader-owned SQL references table `{}`:\n         {paths}\n       fix: update those queries through their typed contracts, then rerun this exact cutover",
        cutover.current.as_str()
    )
    .into())
}

fn declares_table(sql: &str, table: &str) -> bool {
    let normalized = sql
        .to_ascii_lowercase()
        .replace(['\"', '\n', '\r', '\t'], " ");
    [
        format!("create table {table}"),
        format!("create table public.{table}"),
        format!("create table if not exists {table}"),
        format!("create table if not exists public.{table}"),
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

pub(super) fn complete_storage_rename(
    run: &Run,
    selector: &str,
    campaign_text: &str,
    old_version_retired: bool,
    force: bool,
) -> Result<Outcome> {
    let campaign = jails_protocol::lifecycle::RenameCampaignId::parse_hex(campaign_text)?;
    if !old_version_retired {
        return Err("rolling storage completion requires `--old-version-retired`.\n       fix: retire the old application version, then repeat the command with the attestation".into());
    }
    let project = run.project();
    let store = observed(project)?;
    let lifecycle = store
        .lifecycles()
        .iter()
        .find(|lifecycle| {
            lifecycle.expected_path.qualified() == selector
                || matches!(&lifecycle.entity, EntityId::Intent(id) if id.name.as_str().eq_ignore_ascii_case(selector.rsplit('.').next().unwrap_or(selector)))
        })
        .ok_or_else(|| {
            format!(
                "no managed resource matches `{selector}`.\n       fix: inspect `jails resource status`, then use its exact current identity"
            )
        })?;
    let jails_protocol::lifecycle::ResourceState::RenamePending {
        campaign: held_campaign,
        current_table,
        target_table,
        code_stage_receipt,
        ..
    } = &lifecycle.state
    else {
        return Err(format!(
            "`{selector}` has no active rolling rename campaign.\n       fix: inspect `jails resource status {selector}` before completing storage"
        )
        .into());
    };
    if *held_campaign != campaign {
        return Err(format!(
            "campaign `{campaign_text}` is stale or belongs to another resource.\n       fix: use campaign `{}` from `jails resource status {selector}`",
            held_campaign.to_hex()
        )
        .into());
    }
    let generated = jails_generate::generate::rename_table_change(
        project,
        current_table.as_str(),
        target_table.as_str(),
    )?;
    let [artifact] = generated.files.as_slice() else {
        return Err("table cutover generator did not produce exactly one migration.\n       fix: restore the built-in migration renderer and retry".into());
    };
    let migration = crate::route::relative_path(project, &artifact.path)?;
    let sql_sources = sql_sources(project)?;
    let cutover = CutoverPlan {
        current: current_table.clone(),
        target: target_table.clone(),
        migration: migration.clone(),
        artifact: artifact.clone(),
        sql_sources,
    };

    let mut reads = capture::capability_reads()?
        .directory(ProjectPath::parse("src/main/resources/db/migration")?)
        .file(migration.clone());
    for source in &cutover.sql_sources {
        reads = reads.file(source.clone());
    }
    for directory in walked_directories(&cutover.sql_sources) {
        reads = reads.directory(directory);
    }
    let mut java_sources = Vec::new();
    for absolute in jails_java::java::source_files(&project.root().join("src")) {
        let relative = crate::route::relative_path(project, &absolute)?;
        reads = reads.file(relative.clone());
        java_sources.push(relative);
    }
    for directory in walked_directories(&java_sources) {
        reads = reads.directory(directory);
    }
    let (snapshot, _) = capture::projected(project, &reads)?;
    validate_cutover_sql(&snapshot, &cutover)?;

    let mut change = DesiredChange::maintenance(MaintenanceAttribution::Rename);
    let mut manual_java = Vec::new();
    for source in &java_sources {
        let jails_protocol::snapshot::Captured::Present(file) = snapshot.read(source)? else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&file.bytes) else {
            continue;
        };
        let owned = owned_by(&store, source, &lifecycle.entity);
        let mentions = jails_java::identifier::bounded_mentions(text, current_table.as_str());
        if !owned && mentions > 0 {
            manual_java.push(source.clone());
            continue;
        }
        if !owned {
            continue;
        }
        let (updated, hits) = jails_java::identifier::replace_literal_sql_identifier(
            text,
            current_table.as_str(),
            target_table.as_str(),
        );
        if hits == 0 {
            continue;
        }
        let key = ResourceKey::WholeFile(source.clone());
        let owners = store
            .ledger
            .iter()
            .flat_map(|ledger| ledger.resources.iter())
            .find(|row| row.key == key)
            .map(|row| row.owners.clone())
            .ok_or_else(|| {
                format!(
                    "owned source `{source}` has no durable resource row.\n       fix: repair the resource before completing its storage campaign"
                )
            })?;
        change.resources.push(DesiredResource::new(
            key.clone(),
            owners,
            ResourceValue::WholeFile,
        )?);
        change.files.push(DesiredFile {
            path: source.clone(),
            body: DesiredBody::Bytes(updated.into_bytes().into()),
            mode: None,
            resource: Some(key),
            renderer: None,
        });
    }
    if !manual_java.is_empty() {
        let paths = manual_java
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n         ");
        return Err(format!(
            "manual-edit-required: reader-owned Java references table `{}`:\n         {paths}\n       fix: update those references, then rerun this exact campaign completion",
            current_table.as_str()
        )
        .into());
    }

    let migration_key = ResourceKey::WholeFile(migration.clone());
    let owner = ResourceOwner::Entity(lifecycle.entity.clone());
    change.resources.push(DesiredResource::new(
        migration_key.clone(),
        BTreeSet::from([owner]),
        ResourceValue::WholeFile,
    )?);
    change.files.push(DesiredFile {
        path: migration.clone(),
        body: DesiredBody::Bytes(artifact.contents.as_bytes().into()),
        mode: None,
        resource: Some(migration_key),
        renderer: None,
    });
    let applied = store
        .ledger
        .as_ref()
        .and_then(|ledger| ledger.applied.iter().find(|row| row.id == lifecycle.entity))
        .ok_or("rolling campaign entity is missing from the durable ledger.\n       fix: repair the lifecycle before completing storage")?;
    let request = jails_protocol::request::CompleteStorageRenameRequestV1 {
        entity: lifecycle.entity.clone(),
        campaign,
        expected_path: lifecycle.expected_path.clone(),
        current_table: current_table.clone(),
        target_table: target_table.clone(),
        code_stage_receipt: *code_stage_receipt,
        old_version_retired,
    };
    request.validate()?;
    let set = DesiredChangeSet {
        ledger_intent: LedgerIntent {
            generation_before: store.generation(),
            entities_after: vec![jails_protocol::plan::DesiredAppliedEntity {
                id: applied.id.clone(),
                spec: applied.version.spec.clone(),
                owners: applied.owners.clone(),
            }],
            one_shots_after: Vec::new(),
            resources_after: change.resources.clone(),
            entities_removed: Vec::new(),
        },
        ordered: vec![change],
        subject: PlannedSubject::CompleteStorageRename(Box::new(request.clone())),
    };
    set.validate()?;
    let asked = Asked::new(
        CanonicalMutationRequest::CompleteStorageRename(request),
        &["rename", "storage"],
        vec![selector.to_string()],
        BTreeMap::from([("complete".to_string(), vec![campaign_text.to_string()])]),
        BTreeSet::from_iter(
            [
                Some("old-version-retired".to_string()),
                force.then(|| "force".to_string()),
            ]
            .into_iter()
            .flatten(),
        ),
    );
    println!(
        "physical-table-cutover: {} -> {} ({migration})",
        current_table.as_str(),
        target_table.as_str()
    );
    commit_set(run, set, &reads, &asked)
}
