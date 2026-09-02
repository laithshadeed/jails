//! `g association <Name> <child>=<parent> --on <Child> --yields <Parent>`.
//!
//! The syntax editor in front of `emit_sql::relation`, which derives the
//! foreign key, its referential actions and the index from a declared
//! `AppModel.relations` entry.
//!
//! **The declaration lives in the child**, because the foreign key does. A
//! relation named on the parent would read as ownership and compile to a
//! column on the wrong table -- `map <child> -> <parent>` says which way round
//! it goes and the block's position says whose column it is.
//!
//! Both sides are resolved against the model before a byte is written. A
//! mapping naming a column that does not exist would otherwise reach
//! `flyway migrate`, which is the furthest possible point from where the
//! mistake was made -- the same rule `search`'s field list follows.

use super::{MODEL_PATH, parse, read_model};
use crate::Invocation;
use crate::cli::GenerateArgs;
use crate::model_generate::{PreparedMutation, finish_generation};
use crate::model_resource::java_to_label;
use jails_model::{Entity, Facet, ModelPatch};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(super) fn run(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    reject_unsupported_options(&args)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model(&invocation)?;
    let current_model = parse(&current_source)?;
    let child = stored(
        &current_model,
        args.strategy_on.as_deref().ok_or_else(|| {
            Failure::Told(format!(
                "canonical association `{}` needs its child resource\n       fix: pass `--on <Child>` -- the foreign key column is the child's",
                args.name
            ))
        })?,
        "child",
    )?;
    let parent = stored(
        &current_model,
        args.strategy_yields.as_deref().ok_or_else(|| {
            Failure::Told(format!(
                "canonical association `{}` needs its parent resource\n       fix: pass `--yields <Parent>`",
                args.name
            ))
        })?,
        "parent",
    )?;
    let label = java_to_label(&args.name);
    let mappings = mappings(&args, child, parent)?;

    // **lowerCamel, because a relation is a member and not a type.** The
    // familiar spelling capitalises -- `g association Owner` -- and JDL v1
    // refuses that with a diagnostic the reader would have to translate back
    // into "type the name differently to a command whose other arguments are
    // type names".
    let member = lower_first(&args.name);
    let declaration = format!(
        "\n  relation {member} to {} {{\n{}\n  }}",
        parent.names.java_type,
        mappings
            .iter()
            .map(|(local, remote)| format!("    map {local} -> {remote}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    // **Declaring the same association twice is a no-op, not a duplicate.**
    // Every other canonical frontend is idempotent -- a second `g record` with
    // the same shape writes nothing -- and a manifest replayed a second time
    // must not produce "a relation name is declared more than once in this
    // entity". Returned early rather than prepared as a no-op patch, because
    // re-issuing `AddRelation` fails on the id: a relation is added rather
    // than reconciled. Identity is the child and the member name; a
    // *different* mapping under the same name is still the collision the
    // parser refuses, which is what it is for.
    if current_model
        .relations
        .values()
        .any(|relation| relation.label == label && relation.child == child.id)
    {
        crate::model_generate::report_already_declared(&args.name);
        return Ok(());
    }
    let next_source = jails_model::insert_jdl_entity_member(
        &current_source,
        &child.names.java_type,
        "relation",
        &declaration,
    )
    .map_err(super::jdl_edit_failure)?;
    let next_model = parse(&next_source)?;
    let relation = next_model
        .relations
        .values()
        .find(|relation| relation.label == label && relation.child == child.id)
        .cloned()
        .ok_or_else(|| {
            Failure::Told(format!(
                "association `{}` did not link\n       fix: check that `{}` and `{}` are both declared in `{MODEL_PATH}`",
                args.name, child.label, parent.label
            ))
        })?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-relation",
        "relation": relation,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::AddRelation(relation),
        patch_bytes,
        authored_migration: None,
    })
}

/// One entity, resolved and confirmed to have a table.
///
/// A foreign key is a constraint between two *tables*, so a side with no
/// repository has nothing for it to point at -- and the reader's fix is a
/// projection on the entity, not a change here.
fn stored<'a>(model: &'a jails_model::AppModel, name: &str, side: &str) -> Result<&'a Entity> {
    let label = java_to_label(name);
    let entity = model
        .entities
        .values()
        .find(|entity| entity.label == label || entity.names.java_type == name)
        .ok_or_else(|| {
            Failure::Told(format!(
                "`{name}` does not name a canonical entity\n       fix: generate the {side} resource first"
            ))
        })?;
    if !entity.active || !entity.facets.contains(&Facet::Repository) {
        return Err(Failure::Told(format!(
            "association {side} `{name}` has no table for a foreign key to reach\n       fix: run `jails g repo {name}`, or scaffold it"
        )));
    }
    Ok(entity)
}

/// The `child=parent` pairs, resolved against both entities.
fn mappings(args: &GenerateArgs, child: &Entity, parent: &Entity) -> Result<Vec<(String, String)>> {
    if args.fields.is_empty() {
        return Err(Failure::Told(format!(
            "canonical association `{}` needs at least one column pair\n       fix: pass `<childField>=<parentField>`, for example `ownerId=id`",
            args.name
        )));
    }
    args.fields
        .iter()
        .map(|pair| {
            let (local, remote) = pair.split_once('=').ok_or_else(|| {
                Failure::Told(format!(
                    "`{pair}` is not a column pair\n       fix: write `<childField>=<parentField>`, for example `ownerId=id`"
                ))
            })?;
            Ok((field(child, local)?, field(parent, remote)?))
        })
        .collect()
}

/// One field, spelled the way the entity's own field list spells it.
///
/// The Java member rather than the stable label: both resolve, and the
/// declaration sits three lines under `ownerId: uuid` in the same block, so
/// writing `map owner_id -> id` there would read as a different field.
fn field(entity: &Entity, name: &str) -> Result<String> {
    let label = java_to_label(name);
    if let Some(field) = entity.fields.iter().find(|field| field.label == label) {
        return Ok(field.names.java_member.clone());
    }
    Err(Failure::Told(format!(
        "`{name}` is not a field on `{}`\n       fix: name a declared component of that entity",
        entity.label
    )))
}

fn reject_unsupported_options(args: &GenerateArgs) -> Result<()> {
    let unsupported = args.timestamps
        || args.package.is_some()
        || args.default_literal.is_some()
        || args.backfill_file.is_some()
        || !args.indexes.is_empty()
        || !args.uniques.is_empty()
        || args.via.is_some()
        || args.order_by.is_some()
        || args.limit.is_some()
        || args.on_conflict.is_some()
        || args.path.is_some()
        || args.select.is_some()
        || !args.set.is_empty()
        || args.if_match.is_some()
        || !args.bind.is_empty()
        || args.method.is_some()
        || args.consumes.is_some();
    if unsupported {
        return Err(Failure::Told(
            "a canonical association is two entities and the columns between them\n       fix: run `jails g association <Name> <childField>=<parentField> --on <Child> --yields <Parent>`"
                .to_string(),
        ));
    }
    Ok(())
}

/// The lowerCamel spelling of a familiar, capitalised generator name.
fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
