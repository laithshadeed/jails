//! Familiar generators that add one projection facet to an existing entity.

use super::{MODEL_PATH, parse, read_model};
use crate::Invocation;
use crate::cli::GenerateArgs;
use crate::model_generate::{PreparedMutation, finish_generation};
use crate::model_resource::java_to_label;
use jails_model::{EntityId, Facet, ModelPatch};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Factory,
    Dto,
    Repository,
}

impl Kind {
    fn facet(self) -> Facet {
        match self {
            Self::Factory => Facet::Factory,
            Self::Dto => Facet::Dto,
            Self::Repository => Facet::Repository,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Factory => "factory",
            Self::Dto => "dto",
            Self::Repository => "repository",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Self::Factory => "@factory",
            Self::Dto => "@dto",
            Self::Repository => "@repository",
        }
    }
}

pub(super) fn run(args: GenerateArgs, invocation: Invocation, kind: Kind) -> Result<()> {
    reject_unsupported_options(&args, kind)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model()?;
    let current_model = parse(&current_source)?;
    let label = java_to_label(&args.name);
    let entity_id = EntityId::parse(format!("ent_{label}"))
        .map_err(|error| Failure::Told(format!("could not assign entity identity: {error}")))?;
    let entity = current_model.entity(&entity_id).ok_or_else(|| {
        Failure::Told(format!(
            "no canonical `{}` record exists\n       fix: generate the record or scaffold first, then run `jails g {} {}`",
            args.name,
            kind.name(),
            args.name
        ))
    })?;
    if !entity.active || !entity.facets.contains(&Facet::Record) {
        return Err(Failure::Told(format!(
            "canonical `{}` is not an active record\n       fix: revive or generate the record before adding its {}",
            args.name,
            kind.name()
        )));
    }
    let facet = kind.facet();
    if entity.facets.contains(&facet) {
        return finish_generation(PreparedMutation {
            name: args.name,
            invocation,
            model_path,
            current_source: current_source.clone(),
            current_model,
            next_source: current_source,
            patch: ModelPatch::Batch(Vec::new()),
            patch_bytes: br#"{"kind":"batch","patches":[]}"#.to_vec(),
        });
    }
    let next_source = set_marker(
        &current_source,
        &entity.names.java_type,
        kind.marker(),
        true,
    )?;
    let next_model = parse(&next_source)?;
    let next = next_model.entity(&entity_id).ok_or_else(|| {
        Failure::Told(format!(
            "{} facet for `{entity_id}` did not link",
            kind.name()
        ))
    })?;
    if !next.facets.contains(&facet) {
        return Err(Failure::Told(format!(
            "{} facet for `{entity_id}` did not link\n       fix: keep `{}` on the entity header",
            kind.name(),
            kind.marker()
        )));
    }
    let patch = ModelPatch::AddFacet {
        entity: entity_id.clone(),
        facet,
    };
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-facet",
        "entity": entity_id,
        "facet": kind.name(),
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch,
        patch_bytes,
    })
}

pub(crate) fn set_marker(
    source: &str,
    entity_java_name: &str,
    marker: &str,
    enabled: bool,
) -> Result<String> {
    let mut byte_offset = 0;
    for line in source.split_inclusive('\n') {
        let declaration = line.split("//").next().unwrap_or_default().trim();
        if declaration.starts_with("entity ") && declaration.ends_with('{') {
            let name = declaration["entity ".len()..]
                .split_whitespace()
                .next()
                .unwrap_or_default();
            if name == entity_java_name {
                let present = declaration.split_whitespace().any(|word| word == marker);
                if present == enabled {
                    return Ok(source.to_string());
                }
                let mut rewritten = line.to_string();
                if enabled {
                    let brace = rewritten.find('{').ok_or_else(|| {
                        Failure::Told(format!(
                            "the JDL entity `{entity_java_name}` has no opening brace\n       fix: keep the entity header as `entity {entity_java_name} {{` and retry"
                        ))
                    })?;
                    rewritten.insert_str(brace, &format!("{marker} "));
                } else {
                    rewritten = rewritten.replacen(&format!(" {marker}"), "", 1);
                }
                let mut next = source.to_string();
                next.replace_range(byte_offset..byte_offset + line.len(), &rewritten);
                return Ok(next);
            }
        }
        byte_offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL entity `{entity_java_name}`\n       fix: keep it as a top-level `entity {entity_java_name} {{ ... }}` block and retry"
    )))
}

fn reject_unsupported_options(args: &GenerateArgs, kind: Kind) -> Result<()> {
    let unsupported = !args.fields.is_empty()
        || args.timestamps
        || args.package.is_some()
        || args.default_literal.is_some()
        || args.backfill_file.is_some()
        || !args.indexes.is_empty()
        || args.strategy_on.is_some()
        || args.strategy_yields.is_some()
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
        return Err(Failure::Told(format!(
            "a canonical {} derives every field from its entity and accepts only the record name\n       fix: run `jails g {} Name` without fields or projection flags",
            kind.name(),
            kind.name()
        )));
    }
    Ok(())
}
