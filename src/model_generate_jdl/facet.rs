//! Familiar generators that add one projection facet to an existing entity.

use super::MODEL_PATH;
use crate::Invocation;
use crate::cli::GenerateArgs;
use crate::model_command::parse;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::field_syntax::java_to_label;
use jails_model::{EntityId, Evolution, Facet};
use jails_support::{Failure, Result};

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
        let label = jails_model::field_syntax::java_to_label(name);
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
                "full-text search indexes text, and `{}` on `{}` is not.\n       fix: name a string component, or add a plain index with `jails entity index add {} {}`",
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
    let current = crate::model_command::Current::load(&invocation)?;
    let label = java_to_label(&args.name);
    let entity_id = EntityId::parse(format!("ent_{label}"))
        .map_err(|error| Failure::Told(format!("could not assign entity identity: {error}")))?;
    let entity = current.model.entity(&entity_id).ok_or_else(|| {
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
            "`{}` is not an active record\n       fix: revive or generate the record before adding its {}",
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
            &current.model,
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
        if let Some(existing) = declared_arguments(&current.source, &entity.names.java_type, kind)?
            && existing != arguments
        {
            return Err(Failure::Told(format!(
                "`{}` already declares `{}{existing}`\n       fix: edit `{MODEL_PATH}` and add the migration by hand, or keep the current fields -- the indexed set is a generated column, so changing it is a migration jails does not write",
                args.name,
                kind.name()
            )));
        }
        return finish_generation(PreparedMutation {
            name: args.name,
            invocation,
            next_source: current.source.clone(),
            current,
            evolution: Evolution::none(),
            authored_migration: None,
            reader_paths: Vec::new(),
        });
    }
    let next_source = set_projection(
        &current.source,
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
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        current,
        next_source,
        evolution: Evolution::none(),
        authored_migration: None,
        reader_paths: Vec::new(),
    })
}

/// Add every facet an entity does not already carry, as one transition.
///
/// **A scaffold over an existing record is an addition**, the same way one
/// more field is: `g record Post` then `g scaffold Post` asks for the four
/// projections the record has not got, and refusing it as "already declared
/// with a different shape" would make the two commands unusable in the order
/// a reader would type them.
pub(super) fn add_facets(
    source: &str,
    entity: &jails_model::Entity,
    wanted: &std::collections::BTreeSet<Facet>,
) -> Result<Option<String>> {
    let missing = wanted
        .iter()
        .filter(|facet| !entity.facets.contains(facet))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(None);
    }
    let mut next = source.to_string();
    // **`service` and `http` have no marker of their own**, because a scaffold
    // is the one profile that declares them -- so a request that wants either
    // is asking for `use scaffold`, which supplies the rest of the profile it
    // does not already have.
    let profile = missing
        .iter()
        .any(|facet| matches!(facet, Facet::Service | Facet::Http));
    if profile {
        next = set_projection(&next, &entity.names.java_type, "@scaffold", "")?;
    }
    for facet in missing {
        if !profile && let Some(marker) = marker_of(facet) {
            next = set_projection(&next, &entity.names.java_type, marker, "")?;
        }
    }
    Ok(Some(next))
}

/// Widen a declared closed set, when the request extends it in order.
///
/// **A widened enum is an addition, exactly as a new facet is.** `g enum
/// Status OPEN CLOSED` then `g enum Status OPEN CLOSED PENDING` is how a
/// reader adds a constant, and refusing it as "already declared with a
/// different shape" would leave no command that can, only the advice to
/// hand-edit `.jails/model.jdl`.
///
/// The accepted constants must be a *prefix* of the request. A Java enum's
/// ordinal is ABI, so appending is safe and inserting is not; and a constant
/// that left is a narrowing, which the compiler refuses against the rows a
/// live table may hold.
pub(super) fn widen_enum(
    source: &str,
    entity: &jails_model::Entity,
    requested: &[jails_model::EnumConstant],
) -> Result<Option<String>> {
    if requested == entity.enum_constants {
        return Ok(None);
    }
    // **A constant that left is refused here, in the compiler's words.** The
    // frontend sees the narrowing first, so letting it through to be caught
    // during lowering is not an option -- and writing the sentence twice is
    // how two refusals for one situation come to disagree.
    let removed = entity
        .enum_constants
        .iter()
        .filter(|constant| {
            !requested
                .iter()
                .any(|kept| kept.java_name == constant.java_name)
        })
        .map(|constant| constant.java_name.as_str())
        .collect::<Vec<_>>();
    if !removed.is_empty() {
        return Err(jails_project::diagnosed(
            jails_compiler::enum_narrowing_refusal(
                &entity.names.java_type,
                &entity.enum_constants,
                &removed,
            ),
        ));
    }
    if requested.len() <= entity.enum_constants.len() {
        return Ok(None);
    }
    let (head, tail) = requested.split_at(entity.enum_constants.len());
    if head != entity.enum_constants {
        return Ok(None);
    }
    let mut next = source.to_string();
    for constant in tail {
        let member = match &constant.wire_name {
            Some(wire) => format!("  {} = {wire}", constant.java_name),
            None => format!("  {}", constant.java_name),
        };
        next = jails_model::insert_jdl_enum_constant(&next, &entity.names.java_type, &member)
            .map_err(super::jdl_edit_failure)?;
    }
    Ok(Some(next))
}

/// The `use` marker that declares one facet, where a marker declares it.
fn marker_of(facet: Facet) -> Option<&'static str> {
    match facet {
        Facet::Repository => Some("@repository"),
        Facet::Service => Some("@service"),
        Facet::Http => Some("@http"),
        Facet::Dto => Some("@dto"),
        Facet::Factory => Some("@factory"),
        Facet::Seed => Some("@seed"),
        Facet::Search => Some("@search"),
        Facet::Record | Facet::Enum | Facet::Events => None,
    }
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
        // The one profile marker: it declares the repository, the service, the
        // DTO and the HTTP surface together, which is what makes `service` and
        // `http` reachable at all.
        "@scaffold" => Ok("scaffold"),
        _ => Err(Failure::Told(format!(
            "unsupported JDL v1 projection marker `{marker}`.\n       fix: use scaffold, factory, dto, repository, seed, or search through its typed frontend"
        ))),
    }
}

pub(crate) fn set_marker(
    source: &str,
    entity_java_name: &str,
    marker: &str,
    enabled: bool,
) -> Result<String> {
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
    remove_projection(source, entity_java_name, projection)
}

fn remove_projection(source: &str, entity_java_name: &str, projection: &str) -> Result<String> {
    let mut edited = source.to_string();
    loop {
        let cst = jails_model::parse_jdl_cst(&edited).map_err(super::jdl_edit_failure)?;
        let owner = jails_model::field_syntax::java_to_label(entity_java_name);
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
            "`{}` needs the components to index
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
        if kind.takes_fields() {
            return Err(Failure::Told(format!(
                "`{}` accepts only the record name and fields to index\n       fix: run `jails g {} Name <fields...>` without facet flags",
                kind.name(),
                kind.name()
            )));
        }
        return Err(Failure::Told(format!(
            "a {} derives every field from its entity and accepts only the record name\n       fix: run `jails g {} Name` without fields or facet flags",
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
