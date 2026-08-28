//! Familiar standalone main/test CLI syntax over one source-unit node.

use super::{MODEL_PATH, append_declaration, parse, read_model};
use crate::Invocation;
use crate::cli::GenerateArgs;
use crate::generate::ArtifactKind;
use crate::model_generate::{PreparedMutation, finish_generation};
use crate::model_resource::java_to_label;
use jails_model::{ModelPatch, StableId, UnitId, UnitKind};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(super) fn run(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    reject_unsupported_options(&args)?;
    let kind = match args.kind {
        ArtifactKind::Class => ("class", UnitKind::Class),
        ArtifactKind::Interface => ("interface", UnitKind::Interface),
        ArtifactKind::Service => ("service", UnitKind::Service),
        ArtifactKind::Test => ("test", UnitKind::Test),
        ArtifactKind::IntegrationTest => ("integration-test", UnitKind::IntegrationTest),
        ArtifactKind::Sealed => ("sealed", UnitKind::Sealed),
        ArtifactKind::Strategy => ("strategy", UnitKind::Strategy),
        ArtifactKind::Controller => ("controller", UnitKind::Controller),
        _ => unreachable!("source-unit generation accepts standalone source kinds"),
    };
    let stem = jails_generate::generate::strip_redundant_suffix(args.kind, &args.name);
    let label = java_to_label(&stem);
    let unit_id =
        UnitId::parse(format!("unit_{}_{}", kind.0.replace('-', "_"), label)).map_err(|error| {
            Failure::Told(format!("could not assign source-unit identity: {error}"))
        })?;
    let variants = if matches!(args.kind, ArtifactKind::Sealed | ArtifactKind::Strategy) {
        sealed_variants(&args.fields)?
    } else {
        Vec::new()
    };
    let declaration = declaration(kind.0, &stem, &variants, unit_id.as_str(), &args);
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model()?;
    let current_model = parse(&current_source)?;
    let requested = requested_unit(&current_model, &declaration, &unit_id)?;
    if let Some(existing) = current_model.units.get(&unit_id) {
        if existing == &requested {
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
        if matches!(
            existing.kind,
            UnitKind::Sealed | UnitKind::Strategy | UnitKind::Controller
        ) && existing.kind == requested.kind
            && existing.label == requested.label
            && existing.java_type == requested.java_type
            && existing.java_package == requested.java_package
        {
            let next_source = replace_declaration(&current_source, unit_id.as_str(), &declaration)?;
            let patch_bytes = serde_json::to_vec(&json!({
                "kind": "replace-source-unit",
                "unit": requested,
            }))
            .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
            return finish_generation(PreparedMutation {
                name: args.name,
                invocation,
                model_path,
                current_source,
                current_model,
                next_source,
                patch: ModelPatch::ReplaceUnit(requested),
                patch_bytes,
            });
        }
        return Err(Failure::Told(format!(
            "canonical {} `{}` is already declared with a different shape.\n       fix: remove it explicitly before changing its package or kind",
            kind.0, args.name
        )));
    }
    let next_source = append_declaration(current_source.clone(), &declaration);
    let next_model = parse(&next_source)?;
    let unit = next_model
        .units
        .get(&unit_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new source unit `{unit_id}` did not link")))?;
    debug_assert_eq!(unit.kind, kind.1);
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "add-source-unit",
        "unit": unit,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::AddUnit(unit),
        patch_bytes,
    })
}

fn reject_unsupported_options(args: &GenerateArgs) -> Result<()> {
    let source_variants = matches!(args.kind, ArtifactKind::Sealed | ArtifactKind::Strategy);
    let strategy_types = args.kind == ArtifactKind::Strategy;
    let controller = args.kind == ArtifactKind::Controller;
    let unsupported = (!source_variants && !args.fields.is_empty())
        || args.timestamps
        || args.default_literal.is_some()
        || args.backfill_file.is_some()
        || !args.indexes.is_empty()
        || (!strategy_types && !controller && args.strategy_on.is_some())
        || (!strategy_types && !controller && args.strategy_yields.is_some())
        || args.via.is_some()
        || args.order_by.is_some()
        || args.limit.is_some()
        || args.on_conflict.is_some()
        || (!controller && args.path.is_some())
        || args.select.is_some()
        || !args.set.is_empty()
        || args.if_match.is_some()
        || !args.bind.is_empty()
        || (!controller && args.method.is_some())
        || (!controller && args.consumes.is_some())
        || (strategy_types && args.package.is_some());
    if unsupported {
        return Err(Failure::Told(
            "this canonical source-unit kind received unrelated generator flags.\n       fix: use only its name and supported source-unit options"
                .to_string(),
        ));
    }
    if strategy_types && args.strategy_on.is_none() {
        return Err(Failure::Told(
            "a canonical strategy needs the type it examines.\n       fix: pass `--on Type`; add `--yields Result` when it returns a value"
                .to_string(),
        ));
    }
    Ok(())
}

fn declaration(
    kind: &str,
    name: &str,
    variants: &[String],
    id: &str,
    args: &GenerateArgs,
) -> String {
    let variants = variants
        .iter()
        .map(|variant| format!(" {variant}"))
        .collect::<String>();
    let package = args
        .package
        .as_deref()
        .map_or_else(String::new, |package| format!(" @package({package})"));
    let on = args
        .strategy_on
        .as_deref()
        .map_or_else(String::new, |value| format!(" @on({value})"));
    let yields = args
        .strategy_yields
        .as_deref()
        .map_or_else(String::new, |value| format!(" @yields({value})"));
    let method = args
        .method
        .map_or_else(String::new, |value| format!(" @method({})", value.label()));
    let path = args
        .path
        .as_deref()
        .map_or_else(String::new, |value| format!(" @path({value})"));
    let consumes = args.consumes.map_or_else(String::new, |value| {
        format!(" @consumes({})", value.label())
    });
    format!("{kind} {name}{variants} @id({id}){package}{on}{yields}{method}{path}{consumes}\n")
}

fn sealed_variants(arguments: &[String]) -> Result<Vec<String>> {
    if arguments.is_empty() {
        return Err(Failure::Told(
            "a sealed type needs at least one variant\n       fix: name one or more variants, e.g. `generate sealed Result Ok Failed`"
                .to_string(),
        ));
    }
    let mut variants = Vec::new();
    for argument in arguments {
        let trimmed = argument.trim();
        let mut characters = trimmed.chars();
        let variant = characters.next().map_or_else(String::new, |first| {
            first.to_ascii_uppercase().to_string() + characters.as_str()
        });
        if variant.is_empty()
            || !variant
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            return Err(Failure::Told(format!(
                "'{argument}' is not a usable variant name\n       fix: use only ASCII letters and digits"
            )));
        }
        if variants.contains(&variant) {
            return Err(Failure::Told(format!(
                "duplicate variant '{variant}'\n       fix: name each sealed variant once"
            )));
        }
        variants.push(variant);
    }
    Ok(variants)
}

fn replace_declaration(source: &str, unit_id: &str, replacement: &str) -> Result<String> {
    let target = format!("@id({unit_id})");
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let code = line.split("//").next().unwrap_or_default();
        if code.contains(&target) {
            let indent = &line[..line.len() - line.trim_start().len()];
            let comment = line.find("//").map_or("", |at| line[at..].trim_end());
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            let separator = if comment.is_empty() { "" } else { " " };
            let next_line = format!(
                "{indent}{}{separator}{comment}{newline}",
                replacement.trim_end()
            );
            let mut next = source.to_string();
            next.replace_range(offset..offset + line.len(), &next_line);
            return Ok(next);
        }
        offset += line.len();
    }
    Err(Failure::Told(format!(
        "could not find the editable JDL declaration for source unit `{unit_id}`\n       fix: keep the declaration as one top-level line and retry"
    )))
}

fn requested_unit(
    model: &jails_model::AppModel,
    declaration: &str,
    id: &UnitId,
) -> Result<jails_model::SourceUnit> {
    let source = format!(
        "application Comparison @id(project_comparison)\npackage {}\njava {}\ndialect {}\n\n{}",
        model.project.base_package, model.project.java_release, model.project.dialect, declaration
    );
    parse(&source)?
        .units
        .get(id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("requested source unit `{id}` did not link")))
}
