//! Canonical resource projection renames preserve semantic identity and storage.

use crate::Invocation;
use crate::cli::{ExternalRenamePolicy, RenameStrategy};
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{ModelPatch, StableId};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::{Path, PathBuf};

pub(crate) struct Request {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) strategy: RenameStrategy,
    pub(crate) table: Option<String>,
    pub(crate) api: ExternalRenamePolicy,
    pub(crate) route: Option<String>,
}

pub(crate) fn run(request: Request, invocation: Invocation) -> Result<()> {
    // **Two strategies, and the difference is one migration.** Preserving the
    // table renames the Java projection and leaves storage exactly as
    // accepted; a single cutover renames the table too and says so in one
    // forward `alter table ... rename to ...`. Rolling and expand/contract are
    // campaigns -- several plans with an attestation between them -- and a
    // campaign is not something one command can honestly claim to have done.
    let cutover = match request.strategy {
        RenameStrategy::PreserveTable => false,
        RenameStrategy::SingleCutover => true,
        _ => {
            return Err(Failure::Told(
                "canonical resource rename implements `--strategy preserve-table` and `single-cutover`.\n       fix: a rolling or expand/contract rename is a campaign of plans rather than one; run the cutover when the readers are ready"
                    .to_string(),
            ));
        }
    };
    if request.table.is_some() && !cutover {
        return Err(Failure::Told(
            "`--table` would change storage during a preserve-table rename.\n       fix: remove `--table`, or use `--strategy single-cutover` to move the table explicitly"
                .to_string(),
        ));
    }
    if request.api != ExternalRenamePolicy::Preserve || request.route.is_some() {
        return Err(Failure::Told(
            "canonical preserve-table rename keeps external names unchanged.\n       fix: remove `--api rename` and `--route`; API cutover needs its own compatibility policy"
                .to_string(),
        ));
    }

    refuse_reader_java(&invocation.root()?, &request)?;
    let model_path = PathBuf::from(crate::model_command::JDL_PATH);
    let current_source = crate::model_command::read_source(&model_path)?;
    let current_model = crate::model_generate_jdl::parse(&current_source)?;
    let selector = request.from.rsplit('.').next().unwrap_or_default();
    if selector.is_empty() {
        return Err(Failure::Told(
            "canonical resource rename needs a non-empty entity selector.\n       fix: pass an entity label or Java type after `rename resource`"
                .to_string(),
        ));
    }
    let entity = current_model
        .entities
        .values()
        .find(|entity| entity.label == selector || entity.names.java_type == selector)
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical entity `{}` does not exist.\n       fix: name an entity label or Java type declared under `[entities]`",
                request.from
            ))
        })?;
    if entity.names.java_type == request.to {
        return Err(Failure::Told(format!(
            "canonical entity `{}` already projects to `{}`.\n       fix: choose a different Java type name",
            entity.label, request.to
        )));
    }

    // **The route this resource already answers on.** `--api preserve` is the
    // default and means what it says: a rename moves the Java type and, on a
    // cutover, the table, and leaves every external name where callers found
    // it. The projection is named because the pin has to land on the `use`
    // line that declares the surface -- a scaffold's, or a bare `http`'s.
    let accepted_route = (request.api == ExternalRenamePolicy::Preserve
        && entity.facets.contains(&jails_model::Facet::Http))
    .then(|| {
        let projection = if entity.facets.contains(&jails_model::Facet::Repository) {
            "scaffold"
        } else {
            "http"
        };
        (
            projection.to_string(),
            format!("/{}", entity.names.sql_table),
        )
    });
    if cutover {
        refuse_reader_sql(&invocation.root()?, &entity.names.sql_table)?;
    }
    let entity_id = entity.id.clone();
    let sql_table = entity.names.sql_table.clone();
    let next_source = crate::model_generate_jdl::rename_entity(
        &current_source,
        &entity.names.java_type,
        &request.to,
        entity.id.as_str(),
        // Pinned only when the table stays. A cutover lets the SQL name
        // follow the new label, which is what makes the migration below
        // the *whole* of the storage change rather than half of it.
        (!cutover && entity.facets.contains(&jails_model::Facet::Record))
            .then_some(sql_table.as_str()),
        accepted_route
            .as_ref()
            .map(|(projection, route)| (projection.as_str(), route.as_str())),
    )?;
    let next_model = crate::model_generate_jdl::parse(&next_source)?;
    let next_label = next_model
        .entities
        .get(&entity_id)
        .map(|entity| entity.label.clone())
        .ok_or_else(|| {
            Failure::Told(format!(
                "lossless model edit removed entity `{entity_id}`.\n       fix: restore the entity declaration and retry"
            ))
        })?;
    let next_table = next_model
        .entities
        .get(&entity_id)
        .map(|entity| entity.names.sql_table.clone())
        .unwrap_or_else(|| sql_table.clone());
    let patch = ModelPatch::RenameEntityProjection {
        entity: entity_id.clone(),
        label: Some(next_label),
        java: Some(request.to.clone()),
        table: cutover.then(|| next_table.clone()),
        route: accepted_route.as_ref().map(|(_, route)| route.clone()),
    };
    let mut proof = current_model.clone();
    proof.apply(patch.clone()).map_err(Failure::Told)?;
    if next_model != proof {
        return Err(Failure::Told(
            "lossless model edit did not produce the intended semantic rename.\n       fix: restore a canonical entity table and retry"
                .to_string(),
        ));
    }
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "rename-entity-projection",
        "entity": entity_id,
        "java": request.to,
        "table": next_table,
        "storage": if cutover { "single-cutover" } else { "preserved" },
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;

    finish_generation(PreparedMutation {
        name: entity.label.clone(),
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch,
        patch_bytes,
        // The cutover's `alter table ... rename to` is *derived*: the patch
        // states the policy and the compiler emits the statement beside every
        // other schema change, so it lands in the reviewed plan rather than
        // being smuggled in beside it.
        authored_migration: None,
    })
}

/// Refuse a rename that would leave something jails did not write pointing at
/// the old name.
///
/// **The two halves the compiler cannot see.** A reader's own Java importing
/// the type, and a reader's own SQL depending on the table -- both are outside
/// the managed tree, both stop working the moment the rename lands, and both
/// are exactly the case a rename is dangerous for. Reported before anything is
/// written, naming the file, because the fix is a hand edit the reader has to
/// make either way and they would rather make it first than after a broken
/// build.
///
/// The reader's *SQL* is only searched on a cutover: preserving the table
/// changes no name a database knows.
fn refuse_reader_java(root: &Path, request: &Request) -> Result<()> {
    let old_java = request.from.rsplit('.').next().unwrap_or_default();
    let mut java = Vec::new();
    collect(&root.join("src"), "java", &mut java);
    for path in java {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        if jails_support::identifier::replace_identifier(&source, old_java, old_java).1 > 0 {
            let shown = path.strip_prefix(root).unwrap_or(&path).display();
            return Err(Failure::Told(format!(
                "manual-edit-required: `{shown}` names `{old_java}`, and it is yours rather than jails'\n       fix: rename the reference there first, then run this again -- jails does not rewrite source it did not write"
            )));
        }
    }
    Ok(())
}

/// The other half, once the table this rename is about is known.
///
/// **A migration jails did not write is a dependency jails cannot move.** Its
/// own history legitimately names the old table -- that is what history is --
/// so what matters is a file the lock does not seal: a hand-written view,
/// trigger or index over the table this cutover is about to rename.
/// PostgreSQL renames the table and leaves the view pointing at the new one
/// under the old name, or fails outright; either way the reader has to decide,
/// and they can only decide before it happens.
fn refuse_reader_sql(root: &Path, old_table: &str) -> Result<()> {
    let sealed = std::fs::read_to_string(root.join(".jails/compiler.lock.json"))
        .ok()
        .and_then(|source| serde_json::from_str::<serde_json::Value>(&source).ok())
        .and_then(|lock| {
            lock.get("migrations")
                .and_then(serde_json::Value::as_object)
                .map(|migrations| migrations.keys().cloned().collect::<Vec<_>>())
        })
        .unwrap_or_default();
    let mut sql = Vec::new();
    collect(&root.join("src"), "sql", &mut sql);
    for path in sql {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if sealed.iter().any(|name| name == &relative) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        if jails_support::identifier::replace_identifier(&source, old_table, old_table).1 == 0 {
            continue;
        }
        let shown = path.file_name().unwrap_or_default().to_string_lossy();
        // **A view is the one jails cannot even name a fix for.** Its
        // definition is stored by the server with the table already resolved,
        // so renaming underneath it produces a view that still works and no
        // longer says what it depends on -- there is no edit to point at,
        // only a decision. Everything else is a statement in a file, and the
        // objects it creates are what a reader scans a refusal for.
        if source.to_ascii_lowercase().contains("create view") {
            return Err(Failure::Told(format!(
                "opaque-dependency: `{shown}` defines a view over `{old_table}`, and jails did not write it\n       fix: drop and recreate that view against the new table name yourself, or use `--strategy preserve-table` and leave storage where it is"
            )));
        }
        let objects = created_object_names(&source);
        let named = if objects.is_empty() {
            String::new()
        } else {
            format!(" ({})", objects.join(", "))
        };
        return Err(Failure::Told(format!(
            "manual-edit-required: `{shown}`{named} names `{old_table}`, and it is yours rather than jails'\n       fix: rewrite it for the new table name first, then run this again -- or use `--strategy preserve-table` and leave storage where it is"
        )));
    }
    Ok(())
}

/// The names a `create <kind> <name>` statement introduces.
///
/// Enough to say *which* index or constraint is about to be orphaned, which is
/// what a reader scans a refusal for. Deliberately shallow: a hint beside the
/// filename, not an understanding of the SQL.
fn created_object_names(source: &str) -> Vec<String> {
    const NOISE: [&str; 11] = [
        "unique",
        "index",
        "table",
        "or",
        "replace",
        "materialized",
        "if",
        "not",
        "exists",
        "trigger",
        "sequence",
    ];
    let mut found = Vec::new();
    for line in source.lines() {
        let Some(at) = line.to_ascii_lowercase().find("create ") else {
            continue;
        };
        let mut words = line[at + "create ".len()..].split_whitespace();
        let mut name = words.next().unwrap_or_default();
        while NOISE.contains(&name.to_ascii_lowercase().as_str()) {
            name = words.next().unwrap_or_default();
        }
        let name = name.trim_end_matches(['(', ';']);
        if !name.is_empty() {
            found.push(name.to_string());
        }
    }
    found
}

/// Every file under `directory` with this extension, recursively.
fn collect(directory: &Path, extension: &str, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, extension, into);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            into.push(path);
        }
    }
}
