//! Canonical resource projection renames preserve semantic identity and storage.

use crate::Invocation;
use crate::cli::{ExternalRenamePolicy, RenameStrategy};
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{Evolution, EvolutionStep, StableId};
use jails_support::{Failure, Result};
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
    let current = crate::model_command::Current::load(&invocation)?;
    let selector = entity_selector(&request.from)?;
    let entity = current.model
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
        &current.source,
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
    // The cutover's `alter table ... rename to` is *derived*: the evolution
    // states the move and the compiler emits the statement beside every
    // other schema change, so it lands in the reviewed plan rather than
    // being smuggled in beside it. The table it moves to is what the edited
    // source links to, read once here.
    let evolution = match cutover {
        false => Evolution::none(),
        true => {
            let next_model = crate::model_command::parse(&next_source)?;
            let table = next_model
                .entities
                .get(&entity_id)
                .map(|entity| entity.names.sql_table.clone())
                .ok_or_else(|| {
                    Failure::Told(format!(
                        "lossless model edit removed entity `{entity_id}`.\n       fix: restore the entity declaration and retry"
                    ))
                })?;
            Evolution::one(EvolutionStep::RenameTable {
                entity: entity_id.clone(),
                table,
            })
        }
    };

    finish_generation(PreparedMutation {
        name: entity.label.clone(),
        invocation,
        current,
        next_source,
        evolution,
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

/// Refuse a rename that would leave something jails did not write pointing at
/// the old name.
///
/// **The two halves the compiler cannot see.** A reader's own Java importing
/// the type, and a reader's own SQL depending on the table -- both are outside
/// the managed tree, both stop working the moment the rename lands, and both
/// are exactly the case a rename is dangerous for. Reported before anything is
/// The entity a rename names, refusing a qualified selector rather than
/// reading past it.
///
/// **A dotted selector was accepted and its prefix thrown away.** The syntax
/// comes from a vertical slice -- `billing.Invoice` -- and nothing in the
/// model declares one, so `rsplit('.')` renamed whatever entity carried the
/// last segment and said nothing about the rest. Two projects with an
/// `Invoice` each is exactly the case the prefix was meant to disambiguate,
/// and silently picking one is the worst answer available. A slice is a
/// language construct with a price the spec sets (`docs/00-contracts.md`
/// §6.2); until it is paid, an entity is named by its label or its Java
/// type, and `--package` is how a slice's classes are collapsed into one
/// package today.
fn entity_selector(from: &str) -> Result<&str> {
    if let Some((qualifier, name)) = from.rsplit_once('.') {
        return Err(Failure::Told(format!(
            "canonical resource rename does not take a qualified selector, and `{qualifier}` is not a thing this model declares.\n       fix: name the entity itself -- `{name}` -- or, to move its classes into one package, pass `--package`"
        )));
    }
    if from.is_empty() {
        return Err(Failure::Told(
            "canonical resource rename needs a non-empty entity selector.\n       fix: pass an entity label or Java type after `rename resource`"
                .to_string(),
        ));
    }
    Ok(from)
}

/// written, naming the file, because the fix is a hand edit the reader has to
/// make either way and they would rather make it first than after a broken
/// build.
///
/// The reader's *SQL* is only searched on a cutover: preserving the table
/// changes no name a database knows.
fn refuse_reader_java(root: &Path, request: &Request) -> Result<()> {
    let old_java = entity_selector(&request.from)?;
    // **The reader's files only.** Managed sources sit beside theirs under
    // `src/`, name the old type by construction, and are the rename's to
    // move; the lock says which they are.
    let managed = jails_project::capture::managed_paths(root).map_err(|error| {
        Failure::diagnosed(
            error.code,
            format!("could not read the compiler lock: {error}"),
        )
    })?;
    let mut java = Vec::new();
    collect(&root.join("src"), "java", &mut java);
    for path in java {
        let relative = path.strip_prefix(root).ok().and_then(|relative| {
            jails_contracts::ProjectPath::parse(relative.to_string_lossy().replace('\\', "/")).ok()
        });
        if relative.is_some_and(|relative| managed.contains(&relative)) {
            continue;
        }
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
