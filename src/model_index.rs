//! Canonical composite and ordered index evolution.

use crate::Invocation;
use crate::ResourceIndexCommand;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::field_syntax::java_to_label;
use jails_model::{Evolution, EvolutionStep, Facet, IndexId, StableId};
use jails_support::{Failure, Result};
use std::collections::BTreeSet;

pub(crate) fn run(command: ResourceIndexCommand, invocation: Invocation) -> Result<()> {
    match command {
        ResourceIndexCommand::Add {
            entity,
            columns,
            package,
        } => crate::model_command::ensure_owned(invocation.clone())
            .and_then(|()| add(entity, columns, package, invocation)),
        ResourceIndexCommand::Remove {
            entity,
            columns,
            confirm_index,
            package,
        } => crate::model_command::ensure_owned(invocation.clone())
            .and_then(|()| remove(entity, columns, confirm_index, package, invocation)),
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
            "entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    let current = crate::model_command::Current::load(&invocation)?;
    if !current
        .model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        return Err(Failure::Told(
            "index evolution needs an accepted database schema.\n       fix: add the `db` capability before evolving an existing table"
                .to_string(),
        ));
    }
    let entity_label = java_to_label(&entity_name);
    let entity = current.model
        .entities
        .values()
        .find(|entity| entity.label == entity_label || entity.names.java_type == entity_name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "entity `{entity_name}` does not exist.\n       fix: name an entity `.jails/model.jdl` declares"
            ))
        })?;
    if !entity.facets.contains(&Facet::Repository) {
        return Err(Failure::Told(format!(
            "entity `{}` has no stored repository facet.\n       fix: add the index to a stored entity",
            entity.label
        )));
    }
    entity.refuse_retired().map_err(Failure::Told)?;
    let canonical = canonical_columns(entity, &columns)?;
    let entity_java_name = entity.names.java_type.clone();
    let signature = canonical.join(",");
    // **The columns are the identity**, which is also what the parser derives
    // off the line, so the member goes in without an `@id` and reads as the
    // reader would have written it. A hash of the same column list named
    // nothing and forced the attribute onto every index.
    let index_id = IndexId::parse(jails_model::jdl_identity::index_id(
        entity.id.as_str(),
        &canonical,
    ))
    .map_err(Failure::Told)?;
    if entity.indexes.contains_key(&index_id) {
        return Err(Failure::Told(format!(
            "index id `{index_id}` already exists on `{}`\n       fix: name a column list the entity does not index yet, or remove the index first",
            entity.id
        )));
    }
    // v1's `field_list` reads `index [ user_id, created_at desc ]` and allows
    // only `@id` and `@map`.
    let member = format!("  index [{}]", canonical.join(", "));
    let next_source =
        crate::model_generate_jdl::index::insert(&current.source, &entity_java_name, &member)?;
    finish_generation(PreparedMutation {
        name: format!("{}.{}", entity_name, signature),
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
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
            "entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    let current = crate::model_command::Current::load(&invocation)?;
    if !current
        .model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        return Err(Failure::Told(
            "index evolution needs an accepted database schema.\n       fix: add the `db` capability before evolving an existing table"
                .to_string(),
        ));
    }
    let entity_label = java_to_label(&entity_name);
    let entity = current.model
        .entities
        .values()
        .find(|entity| entity.label == entity_label || entity.names.java_type == entity_name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "entity `{entity_name}` does not exist.\n       fix: name an entity declared in the application model"
            ))
        })?;
    if !entity.active {
        return Err(Failure::Told(format!(
            "entity `{}` is retired.\n       fix: revive it before evolving its indexes",
            entity.label
        )));
    }
    if !entity.facets.contains(&Facet::Repository) {
        return Err(Failure::Told(format!(
            "entity `{}` has no stored repository facet.\n       fix: remove an index from a stored entity",
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
                "entity `{}` has no index on `{}`.\n       fix: pass the ordered fields used by `entity index add`",
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
        &current.source,
        &entity_java_name,
        index_id.as_str(),
    )?;
    let next_model = crate::model_command::parse(&next_source)?;
    if next_model
        .entities
        .get(&entity_id)
        .is_some_and(|entity| entity.indexes.contains_key(&index_id))
    {
        return Err(Failure::Told(format!(
            "index `{index_id}` remained after editing the model.\n       fix: keep its generated JDL declaration on one line and retry"
        )));
    }
    finish_generation(PreparedMutation {
        name: format!("{}.{}", entity_name, requested),
        invocation,
        current,
        next_source,
        evolution: Evolution::one(EvolutionStep::RemoveIndex {
            index: index_id,
            confirmed_name,
        }),
        authored_migration: None,
        reader_paths: Vec::new(),
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
                    "`{}` is not a index field.\n       fix: use comma-separated `field`, `field asc`, or `field desc` entries",
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
