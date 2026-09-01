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
    Seed,
    /// **The one projection that carries an argument.** Which components are
    /// indexed is a decision, not a derivation: a `tsvector` over every text
    /// column indexes ids and status codes as if they were prose, and the
    /// reader then cannot tell why a search for "active" returns everything.
    Search,
}

impl Kind {
    fn facet(self) -> Facet {
        match self {
            Self::Factory => Facet::Factory,
            Self::Dto => Facet::Dto,
            Self::Repository => Facet::Repository,
            Self::Seed => Facet::Seed,
            Self::Search => Facet::Search,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Factory => "factory",
            Self::Dto => "dto",
            Self::Repository => "repository",
            Self::Seed => "seed",
            Self::Search => "search",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Self::Factory => "@factory",
            Self::Dto => "@dto",
            Self::Repository => "@repository",
            Self::Seed => "@seed",
            Self::Search => "@search",
        }
    }

    /// Whether this projection's field list is its own declaration rather than
    /// a flag it has no use for.
    fn takes_fields(self) -> bool {
        matches!(self, Self::Search)
    }

    /// What this projection does with the record, for the refusal that fires
    /// when there is no record to do it to.
    fn wants(self) -> &'static str {
        match self {
            Self::Factory => "needs the record it builds",
            Self::Dto => "needs the record it carries",
            Self::Repository => "needs the record it stores",
            Self::Seed => "needs the record it seeds",
            Self::Search => "needs the record it searches",
        }
    }
}

/// What a full-text index cannot be built over, named before anything is
/// written.
///
/// **Each of these reaches the linker as a projection prerequisite**, which is
/// true and is about `$.projections.prj_ent_article_search` -- a symbol the
/// reader never typed. The three failures are distinct and each has a
/// different answer, so they are three refusals rather than one.
fn refuse_unindexable(kind: Kind, entity: &jails_model::Entity, fields: &[String]) -> Result<()> {
    if fields.is_empty() {
        let text = entity
            .fields
            .iter()
            .filter(|field| indexable(field))
            .map(|field| field.names.java_member.as_str())
            .collect::<Vec<_>>();
        return Err(Failure::Told(format!(
            "`{} {}` needs the components it indexes: a `tsvector` over every text column indexes ids and status codes as if they were prose.\n       fix: name them -- {}",
            kind.name(),
            entity.names.java_type,
            if text.is_empty() {
                format!(
                    "`{}` has no text components to index",
                    entity.names.java_type
                )
            } else {
                format!(
                    "`jails g search {} {}`",
                    entity.names.java_type,
                    text.join(" ")
                )
            }
        )));
    }
    for token in fields {
        let name = token
            .split_once(':')
            .map_or(token.as_str(), |(name, _)| name);
        let label = crate::model_resource::java_to_label(name);
        let Some(field) = entity.fields.iter().find(|field| field.label == label) else {
            return Err(Failure::Told(format!(
                "`{}` has no component `{name}`.\n       fix: name one of {}",
                entity.names.java_type,
                entity
                    .fields
                    .iter()
                    .map(|field| format!("`{}`", field.names.java_member))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        };
        if !indexable(field) {
            return Err(Failure::Told(format!(
                "full-text search indexes text, and `{}` on `{}` is not.\n       fix: name a string component, or add a plain index with `jails resource index add {} {}`",
                field.names.java_member,
                entity.names.java_type,
                entity.names.java_type,
                field.names.java_member
            )));
        }
    }
    Ok(())
}

/// Whether a component is prose a `tsvector` can hold.
fn indexable(field: &jails_model::Field) -> bool {
    matches!(
        field.ty,
        jails_model::TypeRef::Builtin(jails_model::BuiltinType::String)
    )
}

pub(super) fn run(args: GenerateArgs, invocation: Invocation, kind: Kind) -> Result<()> {
    reject_unsupported_options(&args, kind)?;
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model(&invocation)?;
    let current_model = parse(&current_source)?;
    let label = java_to_label(&args.name);
    let entity_id = EntityId::parse(format!("ent_{label}"))
        .map_err(|error| Failure::Told(format!("could not assign entity identity: {error}")))?;
    let entity = current_model.entity(&entity_id).ok_or_else(|| {
        Failure::Told(format!(
            "`{} {}` {}, and this project declares none called `{}`\n       fix: `jails g record {} <field>:<type>` (or `jails g scaffold {}`) first, then `jails g {} {}`",
            kind.name(),
            args.name,
            kind.wants(),
            args.name,
            args.name,
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
    // **Resolved before the already-declared check, not after.** A typo in a
    // field name would otherwise be reported only on a first run: a second
    // `jails g search Note headlien` on an entity that already searches would
    // take the no-op path and report success over a component that does not
    // exist.
    let arguments = if kind.takes_fields() {
        refuse_unindexable(kind, entity, &args.fields)?;
        let labels = crate::model_generate::operation_field_labels(
            &current_model,
            &entity.label,
            &args.fields,
        )?;
        format!("(fields: [{}])", labels.join(", "))
    } else {
        String::new()
    };
    let facet = kind.facet();
    if entity.facets.contains(&facet) {
        // Re-declaring a *parameterised* projection with different arguments
        // is a change, and there is no path for one: the indexed set is baked
        // into a generated column, so altering it is a migration nothing here
        // writes. Saying so beats a silent no-op that leaves the reader
        // believing the new field is indexed.
        if let Some(existing) = declared_arguments(&current_source, &entity.names.java_type, kind)?
            && existing != arguments
        {
            return Err(Failure::Told(format!(
                "canonical `{}` already declares `{}{existing}`\n       fix: the indexed set is a generated column, so changing it is a migration jails does not write -- edit `{MODEL_PATH}` and add one by hand, or keep the current fields",
                args.name,
                kind.name()
            )));
        }
        return finish_generation(PreparedMutation {
            name: args.name,
            invocation,
            model_path,
            current_source: current_source.clone(),
            current_model,
            next_source: current_source,
            patch: ModelPatch::Batch(Vec::new()),
            patch_bytes: br#"{"kind":"batch","patches":[]}"#.to_vec(),
            authored_migration: None,
        });
    }
    let next_source = set_projection(
        &current_source,
        &entity.names.java_type,
        kind.marker(),
        &arguments,
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
        authored_migration: None,
    })
}

/// Add one projection, with the arguments it carries.
///
/// Separate from [`set_marker`] because only this direction can take an
/// argument: removing `use search(fields: [...])` names the projection, not
/// its fields.
fn set_projection(
    source: &str,
    entity_java_name: &str,
    marker: &str,
    arguments: &str,
) -> Result<String> {
    if arguments.is_empty() {
        return set_marker(source, entity_java_name, marker, true);
    }
    if !super::is_v1_source(source) {
        return Err(Failure::Told(format!(
            "projection `{marker}` carries arguments, which only `jdl 1` can express.\n       fix: run `jails model upgrade --to 1` first"
        )));
    }
    let projection = v1_projection(marker)?;
    jails_model::insert_jdl_entity_member(
        source,
        entity_java_name,
        "use",
        &format!("  use {projection}{arguments}"),
    )
    .map_err(super::jdl_edit_failure)
}

/// The JDL v1 spelling of one projection marker.
fn v1_projection(marker: &str) -> Result<&'static str> {
    match marker {
        "@factory" => Ok("factory"),
        "@dto" => Ok("dto"),
        "@repository" => Ok("repo"),
        "@seed" => Ok("seed"),
        "@search" => Ok("search"),
        _ => Err(Failure::Told(format!(
            "unsupported JDL v1 projection marker `{marker}`.\n       fix: use factory, dto, repository, seed, or search through its typed frontend"
        ))),
    }
}

pub(crate) fn set_marker(
    source: &str,
    entity_java_name: &str,
    marker: &str,
    enabled: bool,
) -> Result<String> {
    if super::is_v1_source(source) {
        let projection = v1_projection(marker)?;
        if enabled {
            return jails_model::insert_jdl_entity_member(
                source,
                entity_java_name,
                "use",
                &format!("  use {projection}"),
            )
            .map_err(super::jdl_edit_failure);
        }
        return remove_projection(source, entity_java_name, projection);
    }
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

fn remove_projection(source: &str, entity_java_name: &str, projection: &str) -> Result<String> {
    let mut edited = source.to_string();
    loop {
        let cst = jails_model::parse_jdl_cst(&edited).map_err(super::jdl_edit_failure)?;
        let owner = crate::model_resource::java_to_label(entity_java_name);
        let Some(member) = cst
            .members
            .iter()
            .find(|member| {
                member.owner == owner
                    && member.kind == "use"
                    && projection_segments(cst.member_text(member))
                        .iter()
                        .any(|segment| projection_name(segment) == projection)
            })
            .cloned()
        else {
            return Ok(edited);
        };
        let original = cst.member_text(&member);
        let newline = if original.ends_with("\r\n") {
            "\r\n"
        } else if original.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        let without_newline = original.trim_end_matches(['\r', '\n']);
        let (code, comment) = without_newline
            .split_once("//")
            .map_or((without_newline, None), |(code, comment)| {
                (code, Some(comment))
            });
        let indentation = &code[..code.len() - code.trim_start().len()];
        let retained = projection_segments(code)
            .into_iter()
            .filter(|segment| projection_name(segment) != projection)
            .collect::<Vec<_>>();
        let replacement = if retained.is_empty() {
            comment.map_or_else(String::new, |comment| {
                format!("{indentation}//{comment}{newline}")
            })
        } else {
            let comment = comment.map_or_else(String::new, |comment| format!(" //{comment}"));
            format!("{indentation}use {}{comment}{newline}", retained.join(", "))
        };
        edited = cst
            .replace_span(member.span, &replacement)
            .map_err(super::jdl_edit_failure)?;
    }
}

fn projection_segments(line: &str) -> Vec<String> {
    let code = line.split_once("//").map_or(line, |(code, _)| code);
    let Some(body) = code.trim().strip_prefix("use ") else {
        return Vec::new();
    };
    let mut segments = Vec::new();
    let mut start = 0;
    let mut depth = 0_u32;
    let mut string = false;
    let mut escaped = false;
    for (offset, character) in body.char_indices() {
        if string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                string = false;
            }
            continue;
        }
        match character {
            '"' => string = true,
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                segments.push(body[start..offset].trim().to_string());
                start = offset + character.len_utf8();
            }
            _ => {}
        }
    }
    segments.push(body[start..].trim().to_string());
    segments
}

fn projection_name(segment: &str) -> &str {
    segment
        .split(|character: char| character == '(' || character.is_whitespace())
        .next()
        .unwrap_or_default()
}

fn reject_unsupported_options(args: &GenerateArgs, kind: Kind) -> Result<()> {
    if kind.takes_fields() && args.fields.is_empty() {
        return Err(Failure::Told(format!(
            "canonical `{}` needs the components to index
       fix: run `jails g {} Name title body` -- indexing every text column would index ids and status codes as prose",
            kind.name(),
            kind.name()
        )));
    }
    let unsupported = (!kind.takes_fields() && !args.fields.is_empty())
        || args.timestamps
        || args.package.is_some()
        || args.default_literal.is_some()
        || args.backfill_file.is_some()
        || !args.indexes.is_empty()
        || !args.uniques.is_empty()
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

/// The arguments an entity's existing projection of this kind carries, or
/// `None` when it has none.
///
/// Read off the source rather than the model because the model normalises:
/// `ProjectionKind::Search { fields }` holds stable field IDs, and comparing
/// those against a freshly rendered label list would be a second projection of
/// one thing. The source is what the reader sees and what the next edit
/// rewrites.
fn declared_arguments(source: &str, entity_java_name: &str, kind: Kind) -> Result<Option<String>> {
    if !kind.takes_fields() {
        return Ok(None);
    }
    let projection = v1_projection(kind.marker())?;
    // Entity-scoped: `use` members are inside the block that names
    // the entity, and a second entity's `use search(...)` must not answer for
    // this one.
    let mut inside = false;
    for line in source.lines() {
        let declaration = line.split("//").next().unwrap_or_default().trim();
        if declaration.starts_with("entity ") {
            let name = declaration
                .strip_prefix("entity ")
                .unwrap_or_default()
                .split(|character: char| character.is_whitespace() || character == '{')
                .next()
                .unwrap_or_default();
            inside = name == entity_java_name;
            continue;
        }
        if inside && declaration == "}" {
            break;
        }
        if !inside {
            continue;
        }
        if let Some(rest) = declaration.strip_prefix("use ")
            && let Some(arguments) = rest.trim().strip_prefix(projection)
        {
            return Ok(Some(arguments.trim().to_string()));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"jdl 1
app Demo {
  pkg com.example.demo
  java 26
  platform spring
  build maven
  storage postgres
}

entity Task {
  use repo, factory, dto // keep projection note
  id: uuid @pk
}
"#;

    #[test]
    fn v1_projection_edits_touch_only_the_selected_use_member() {
        let removed = set_marker(SOURCE, "Task", "@factory", false).unwrap();
        assert!(removed.contains("use repo, dto // keep projection note"));
        assert!(!removed.contains("factory"));
        jails_model::parse_jdl(&removed).unwrap();

        let restored = set_marker(&removed, "Task", "@factory", true).unwrap();
        assert!(restored.contains("use repo, dto // keep projection note"));
        assert!(restored.contains("use factory"));
        jails_model::parse_jdl(&restored).unwrap();
    }
}
