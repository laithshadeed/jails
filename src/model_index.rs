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
    let jdl = invocation.owns_jdl();
    if package.is_some() {
        return Err(Failure::Told(
            "canonical entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    let model_path = PathBuf::from(if jdl {
        crate::model_command::JDL_PATH
    } else {
        crate::model_command::TOML_PATH
    });
    let current_source = crate::model_command::read_source_at(&invocation.root()?, &model_path)?;
    let current_model = parse_model(&current_source, jdl)?;
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
    let index_label = format!("index_{suffix}");
    let index_id = IndexId::parse(format!("idx_{model_label}_{suffix}")).map_err(Failure::Told)?;
    let mut next_source = current_source.clone();
    if jdl {
        // **Two grammars, one file extension.** `.jails/model.jdl` may hold
        // either syntax, and they disagree about a constraint: v1's
        // `field_list` reads `index [ user_id, created_at desc ]` and allows
        // only `@id` and `@map`, while v0 takes parentheses and names the
        // index with `@as`. **Branch on the source, never the filename**: a
        // `.jdl` file holds either syntax, so writing the v0 form for one
        // produces a declaration its own parser rejects -- "a constraint needs
        // a bracketed field list", pointing at the entity's closing brace.
        //
        // A test whose model is v0 syntax in a `.jdl` file cannot catch that;
        // it is the one shape where branching on the filename is right.
        let member = match crate::model_generate_jdl::is_v1_source(&next_source) {
            true => format!(
                "  index [{}] @id({})",
                canonical.join(", "),
                index_id.as_str()
            ),
            false => format!(
                "  index ({}) @id({}) @as({index_label})",
                canonical.join(", "),
                index_id.as_str()
            ),
        };
        next_source =
            crate::model_generate_jdl::index::insert(&next_source, &entity_java_name, &member)?;
    } else {
        if !next_source.ends_with('\n') {
            next_source.push('\n');
        }
        next_source.push_str(&format!(
            "\n[entities.{}.indexes.{index_label}]\nid = {}\ncolumns = {}\n",
            model_label,
            quote(index_id.as_str())?,
            quote_list(&canonical)?,
        ));
    }
    let next_model = parse_model(&next_source, jdl)?;
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
    let jdl = invocation.owns_jdl();
    if package.is_some() {
        return Err(Failure::Told(
            "canonical entities have one stable identity and do not accept a legacy package selector.\n       fix: remove `--package` and name the entity declared in the application model"
                .to_string(),
        ));
    }
    let model_path = PathBuf::from(if jdl {
        crate::model_command::JDL_PATH
    } else {
        crate::model_command::TOML_PATH
    });
    let current_source = crate::model_command::read_source_at(&invocation.root()?, &model_path)?;
    let current_model = parse_model(&current_source, jdl)?;
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
    let entity_model_label = entity.label.clone();
    let entity_java_name = entity.names.java_type.clone();
    let index_id = index.id.clone();
    let index_label = index.label.clone();
    let next_source = if jdl {
        crate::model_generate_jdl::index::remove(
            &current_source,
            &entity_java_name,
            index_id.as_str(),
        )?
    } else {
        jails_model::remove_index_declaration(&current_source, &entity_model_label, &index_label)
            .map_err(Failure::Told)?
    };
    let next_model = parse_model(&next_source, jdl)?;
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

fn parse_model(source: &str, jdl: bool) -> Result<jails_model::AppModel> {
    let parsed = if jdl {
        jails_model::parse_jdl(source)
    } else {
        jails_model::parse_toml(source)
    };
    parsed.map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}

fn quote(value: &str) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| Failure::Told(format!("could not quote model value: {error}")))
}

fn quote_list(values: &[String]) -> Result<String> {
    serde_json::to_string(values)
        .map_err(|error| Failure::Told(format!("could not quote model values: {error}")))
}
