//! `rename resource --strategy single-cutover`: the rename that moves the
//! table.
//!
//! The sibling strategy, preserve-table, is a projection change — the Java
//! moves and the SQL does not. This one changes both, so it plans one forward
//! migration alongside the source moves and has to name every SQL source that
//! referred to the old table, in the same transition, or the project compiles
//! against a table that no longer exists.
//!
//! Planning only: everything here returns a `CutoverPlan`, and nothing is
//! written until the executor applies it. A destination collision or an
//! overlapping edit refuses before any model, migration or source write.

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
    let generated = jails_generate::generate::rename_table_change(
        project,
        current.as_str(),
        target.as_str(),
        &[],
    )?;
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

pub(super) fn prepare_owned_object_renames(
    project: &Project,
    snapshot: &jails_protocol::snapshot::ProjectSnapshot,
    store: &ObservedStore,
    entity: &EntityId,
    cutover: &mut CutoverPlan,
) -> Result<()> {
    use jails_generate::generate::StorageObjectRename;

    let mut declarations = Vec::new();
    for path in &cutover.sql_sources {
        if !ResourceKey::WholeFile(path.clone()).is_migration_history() {
            continue;
        }
        let jails_protocol::snapshot::Captured::Present(file) = snapshot.read(path)? else {
            continue;
        };
        let text = std::str::from_utf8(&file.bytes).map_err(|_| {
            format!(
                "opaque-dependency: migration `{path}` is not UTF-8.\n       \
                 fix: inspect that migration and prove its storage dependencies before retrying."
            )
        })?;
        let generator_owned = owned_by(store, path, entity);
        declarations.extend(
            storage_object_declarations(text)
                .into_iter()
                .map(|(kind, name)| (path.clone(), generator_owned, kind, name)),
        );
    }

    let current_prefix = format!("{}_", cutover.current.as_str());
    let target_prefix = format!("{}_", cutover.target.as_str());
    let declared_names = declarations
        .iter()
        .map(|(_, _, _, name)| name.as_str())
        .collect::<BTreeSet<_>>();
    let mut manual = Vec::new();
    let mut renames = BTreeSet::new();
    for (path, generator_owned, kind, name) in &declarations {
        let Some(suffix) = name.strip_prefix(&current_prefix) else {
            continue;
        };
        let target_name = format!("{target_prefix}{suffix}");
        if declared_names.contains(target_name.as_str()) {
            return Err(format!(
                "target-object-collision: `{target_name}` already exists in migration history.\n       \
                 fix: choose another table name or explicitly reconcile the colliding object."
            )
            .into());
        }
        if !generator_owned {
            manual.push(format!("{path}: {name}"));
            continue;
        }
        renames.insert(StorageObjectRename {
            kind: *kind,
            current: jails_protocol::identity::SqlName::parse(name)?,
            target: jails_protocol::identity::SqlName::parse(&target_name)?,
        });
    }
    if !manual.is_empty() {
        return Err(format!(
            "manual-edit-required: reader-owned storage object names require an explicit accepted operation:\n         {}\n       \
             fix: rename those objects in a reviewed forward migration, then retry the table cutover.",
            manual.join("\n         ")
        )
        .into());
    }

    let generated = jails_generate::generate::rename_table_change(
        project,
        cutover.current.as_str(),
        cutover.target.as_str(),
        &renames.into_iter().collect::<Vec<_>>(),
    )?;
    let [artifact] = generated.files.as_slice() else {
        return Err(concat!(
            "storage cutover generator did not produce exactly one migration.\n       ",
            "fix: restore the built-in migration renderer and retry."
        )
        .into());
    };
    if crate::route::relative_path(project, &artifact.path)? != cutover.migration {
        return Err(concat!(
            "storage object discovery changed the cutover migration path.\n       ",
            "fix: report this as a jails planning bug."
        )
        .into());
    }
    cutover.artifact = artifact.clone();
    Ok(())
}

fn storage_object_declarations(
    sql: &str,
) -> Vec<(jails_generate::generate::StorageObjectKind, String)> {
    use jails_generate::generate::StorageObjectKind;

    let tokens = sql_tokens(sql);
    let mut declarations = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token == "constraint" {
            if let Some(name) = tokens.get(index + 1) {
                declarations.push((StorageObjectKind::Constraint, name.clone()));
            }
            continue;
        }
        if token != "create" {
            continue;
        }
        let mut cursor = index + 1;
        if tokens.get(cursor).is_some_and(|token| token == "unique") {
            cursor += 1;
        }
        let kind = match tokens.get(cursor).map(String::as_str) {
            Some("index") => StorageObjectKind::Index,
            Some("sequence") => StorageObjectKind::Sequence,
            _ => continue,
        };
        cursor += 1;
        if tokens.get(cursor).is_some_and(|token| token == "if")
            && tokens.get(cursor + 1).is_some_and(|token| token == "not")
            && tokens
                .get(cursor + 2)
                .is_some_and(|token| token == "exists")
        {
            cursor += 3;
        }
        if let Some(name) = tokens.get(cursor) {
            declarations.push((kind, name.clone()));
        }
    }
    declarations
}

fn sql_tokens(sql: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = sql.chars().peekable();
    let mut quoted = false;
    while let Some(character) = chars.next() {
        if quoted {
            if character == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                } else {
                    quoted = false;
                }
            }
            continue;
        }
        if character == '\'' {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            quoted = true;
        } else if character == '-' && chars.peek() == Some(&'-') {
            chars.next();
            for rest in chars.by_ref() {
                if rest == '\n' {
                    break;
                }
            }
        } else if character.is_ascii_alphanumeric() || character == '_' {
            current.push(character.to_ascii_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
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
    let mut opaque = Vec::new();
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
            if opaque_dependency_mentions(text, cutover.current.as_str()) {
                opaque.push(path.clone());
            }
        } else if jails_java::identifier::bounded_mentions(text, cutover.current.as_str()) > 0 {
            manual.push(path.clone());
        }
    }
    if !opaque.is_empty() {
        let paths = opaque
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n         ");
        return Err(format!(
            "opaque-dependency: migration history contains a view, routine, trigger, policy, rule, or dynamic SQL that may depend on table `{}`:\n         {paths}\n       fix: prove and migrate those database objects explicitly before retrying this storage cutover",
            cutover.current.as_str()
        )
        .into());
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

fn opaque_dependency_mentions(sql: &str, table: &str) -> bool {
    if jails_java::identifier::bounded_mentions(sql, table) == 0 {
        return false;
    }
    let normalized = sql
        .to_ascii_lowercase()
        .replace(['\"', '\n', '\r', '\t'], " ");
    [
        "create view ",
        "create materialized view ",
        "create function ",
        "create procedure ",
        "create trigger ",
        "create policy ",
        "create rule ",
        "execute ",
        "execute immediate ",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
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
        &[],
    )?;
    let [artifact] = generated.files.as_slice() else {
        return Err("table cutover generator did not produce exactly one migration.\n       fix: restore the built-in migration renderer and retry".into());
    };
    let migration = crate::route::relative_path(project, &artifact.path)?;
    let sql_sources = sql_sources(project)?;
    let mut cutover = CutoverPlan {
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
    prepare_owned_object_renames(project, &snapshot, &store, &lifecycle.entity, &mut cutover)?;

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
            .resources()
            .iter()
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
        .entities()
        .iter()
        .find(|row| row.id == lifecycle.entity)
        .ok_or("rolling campaign entity is missing from durable state.\n       fix: repair the lifecycle before completing storage")?;
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
    let set = super::resource::complete_storage_set(&store, applied, change, request.clone())?;
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
