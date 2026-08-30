//! Canonical composite and ordered index evolution.

use crate::Invocation;
use crate::ResourceIndexCommand;
use crate::model_generate::{PreparedMutation, finish_generation};
use crate::model_resource::java_to_label;
use jails_model::{Facet, IndexId, ModelPatch, StableId};
use jails_support::codec::{hex, sha256};
use jails_support::{Failure, Result};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;

pub(crate) fn owns() -> bool {
    crate::model_command::owns()
}

pub(crate) fn run(command: ResourceIndexCommand, invocation: Invocation) -> Result<()> {
    match command {
        ResourceIndexCommand::Add {
            entity,
            columns,
            package,
        } => {
            if owns() {
                add(entity, columns, package, invocation)
            } else {
                crate::dispatch::mutate(invocation, false, |run| {
                    jails_engine::route::add_index(run, &entity, &columns, package.as_deref())
                })
            }
        }
        ResourceIndexCommand::Remove {
            entity,
            columns,
            confirm_index,
            package,
        } => {
            if owns() {
                remove(entity, columns, confirm_index, package, invocation)
            } else {
                Err(Failure::Told(
                    "index removal is available only in a canonical model project.\n       fix: import the project with `jails model import`, or keep the legacy index migration reader-owned"
                        .to_string(),
                ))
            }
        }
    }
}

pub(crate) fn add(
    entity_name: String,
    columns: String,
    package: Option<String>,
    invocation: Invocation,
) -> Result<()> {
    if package.is_some() {
        return Err(Failure::Told(
            "canonical entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    let model_path = PathBuf::from(crate::model_command::JDL_PATH);
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })?;
    let current_model = parse_model(&current_source)?;
    if !current_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        return Err(Failure::Told(
            "canonical index evolution needs an accepted database schema.\n       fix: add the `db` capability before evolving an existing table"
                .to_string(),
        ));
    }
    let entity_label = java_to_label(&entity_name);
    let entity = current_model
        .entities
        .values()
        .find(|entity| entity.label == entity_label || entity.names.java_type == entity_name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical entity `{entity_name}` does not exist.\n       fix: name an entity declared under `[entities]`"
            ))
        })?;
    if !entity.facets.contains(&Facet::Repository) {
        return Err(Failure::Told(format!(
            "canonical entity `{}` has no stored repository facet.\n       fix: add the index to a stored entity",
            entity.label
        )));
    }
    let canonical = canonical_columns(entity, &columns)?;
    let entity_id = entity.id.clone();
    let model_label = entity.label.clone();
    let entity_java_name = entity.names.java_type.clone();
    let signature = canonical.join(",");
    let suffix = &hex(&sha256(signature.as_bytes()))[..12];
    let index_id = IndexId::parse(format!("idx_{model_label}_{suffix}")).map_err(Failure::Told)?;
    let mut next_source = current_source.clone();
    // **JDL v1 spells an index with a bracketed field list and no
    // `@as`.** This rendered the pre-v1 draft's `index (...) @as(...)`
    // for any JDL source, so on a v1 model -- the format this compiler
    // authors -- the command could never succeed: the re-parse below
    // rejected its own output with "a constraint needs a bracketed field
    // list". Every other frontend asks `is_v1_source`; this one rendered
    // the line itself and never asked. The label is derived from the
    // columns in v1, so only the identity needs pinning.
    next_source = crate::model_generate_jdl::index::insert(
        &next_source,
        &entity_java_name,
        &format!(
            "  index [{}] @id({})",
            canonical.join(", "),
            index_id.as_str()
        ),
    )?;
    let next_model = parse_model(&next_source)?;
    let index = next_model
        .entities
        .get(&entity_id)
        .and_then(|entity| entity.indexes.get(&index_id))
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new index `{index_id}` did not link")))?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-index",
        "entity": entity_id,
        "index": index,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: format!("{}.{}", entity_name, signature),
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::AddIndex {
            entity: entity_id,
            index,
        },
        patch_bytes,
        authored_migration: None,
    })
}

pub(crate) fn remove(
    entity_name: String,
    columns: String,
    confirmed_name: String,
    package: Option<String>,
    invocation: Invocation,
) -> Result<()> {
    if package.is_some() {
        return Err(Failure::Told(
            "canonical entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    let model_path = PathBuf::from(crate::model_command::JDL_PATH);
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })?;
    let current_model = parse_model(&current_source)?;
    if !current_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        return Err(Failure::Told(
            "canonical index evolution needs an accepted database schema.\n       fix: add the `db` capability before evolving an existing table"
                .to_string(),
        ));
    }
    let entity_label = java_to_label(&entity_name);
    let entity = current_model
        .entities
        .values()
        .find(|entity| entity.label == entity_label || entity.names.java_type == entity_name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical entity `{entity_name}` does not exist.\n       fix: name an entity declared in the application model"
            ))
        })?;
    if !entity.active {
        return Err(Failure::Told(format!(
            "canonical entity `{}` is retired.\n       fix: revive it before evolving its indexes",
            entity.label
        )));
    }
    if !entity.facets.contains(&Facet::Repository) {
        return Err(Failure::Told(format!(
            "canonical entity `{}` has no stored repository facet.\n       fix: remove an index from a stored entity",
            entity.label
        )));
    }
    let canonical = canonical_columns(entity, &columns)?;
    let requested = canonical.join(",");
    let index = entity
        .indexes
        .values()
        .find(|index| {
            index
                .columns
                .iter()
                .map(|column| {
                    let field = entity
                        .field(&column.field)
                        .expect("linked indexes reference existing fields");
                    if column.direction == jails_model::IndexDirection::Desc {
                        format!("{} desc", field.label)
                    } else {
                        field.label.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
                == requested
        })
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical entity `{}` has no index on `{}`.\n       fix: pass the exact ordered fields used by `resource index add`",
                entity.label,
                canonical.join(", ")
            ))
        })?;
    if index.sql_name != confirmed_name {
        return Err(Failure::Told(format!(
            "confirmed index `{confirmed_name}` is not `{}`.\n       fix: pass `--confirm-index {}` exactly",
            index.sql_name, index.sql_name
        )));
    }
    let entity_id = entity.id.clone();
    let entity_java_name = entity.names.java_type.clone();
    let index_id = index.id.clone();
    let next_source = crate::model_generate_jdl::index::remove(
        &current_source,
        &entity_java_name,
        index_id.as_str(),
    )?;
    let next_model = parse_model(&next_source)?;
    if next_model
        .entities
        .get(&entity_id)
        .is_some_and(|entity| entity.indexes.contains_key(&index_id))
    {
        return Err(Failure::Told(format!(
            "index `{index_id}` remained after editing the canonical model.\n       fix: keep its generated JDL declaration on one line and retry"
        )));
    }
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "remove-index",
        "entity": entity_id,
        "index": index_id,
        "confirmed_name": confirmed_name,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: format!("{}.{}", entity_name, requested),
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::RemoveIndex {
            entity: entity_id,
            index: index_id,
            confirmed_name,
        },
        patch_bytes,
        authored_migration: None,
    })
}

fn canonical_columns(entity: &jails_model::Entity, columns: &str) -> Result<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut canonical = Vec::new();
    for raw in columns.split(',') {
        let pieces = raw.split_whitespace().collect::<Vec<_>>();
        let (name, descending) = match pieces.as_slice() {
            [name] | [name, "asc"] => (*name, false),
            [name, "desc"] => (*name, true),
            _ => {
                return Err(Failure::Told(format!(
                    "`{}` is not a canonical index field.\n       fix: use comma-separated `field`, `field asc`, or `field desc` entries",
                    raw.trim()
                )));
            }
        };
        let field = entity
            .fields
            .iter()
            .find(|field| {
                field.label == name
                    || field.names.java_member == name
                    || field.names.sql_column == name
            })
            .ok_or_else(|| {
                Failure::Told(format!(
                    "index field `{name}` does not exist on `{}`.\n       fix: name a model label, Java member, or SQL column from that entity",
                    entity.label
                ))
            })?;
        if !seen.insert(field.id.clone()) {
            return Err(Failure::Told(format!(
                "index field `{name}` appears more than once.\n       fix: remove the duplicate entry"
            )));
        }
        canonical.push(if descending {
            format!("{} desc", field.label)
        } else {
            field.label.clone()
        });
    }
    if canonical.is_empty() {
        return Err(Failure::Told(
            "an index needs at least one field.\n       fix: pass a comma-separated field list"
                .to_string(),
        ));
    }
    Ok(canonical)
}

fn parse_model(source: &str) -> Result<jails_model::AppModel> {
    jails_model::parse_jdl(source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}
