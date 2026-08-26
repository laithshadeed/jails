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
