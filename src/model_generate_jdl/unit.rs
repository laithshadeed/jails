//! Familiar standalone main/test CLI syntax over one source-unit node.

use super::component::{
    component_kind, component_stem, legacy_unit_kind, reject_v1_options, replace_v1_declaration,
    v1_declaration,
};
use super::{MODEL_PATH, append_declaration, parse, read_model};
use crate::Invocation;
use crate::cli::GenerateArgs;
use crate::generate::ArtifactKind;
use crate::model_generate::{PreparedMutation, finish_generation};
use crate::model_resource::java_to_label;
use jails_model::{ComponentId, ModelPatch, StableId, UnitId, UnitKind};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(super) fn run(args: GenerateArgs, invocation: Invocation) -> Result<()> {
    let component_kind = component_kind(args.kind)
        .expect("the JDL router sends only closed component kinds to this frontend");
    let legacy_kind = legacy_unit_kind(args.kind);
    let stem = component_stem(args.kind, &args.name)?;
    let variants = if matches!(args.kind, ArtifactKind::Sealed | ArtifactKind::Strategy) {
        sealed_variants(&args.fields)?
    } else {
        Vec::new()
    };
    let model_path = PathBuf::from(MODEL_PATH);
    let current_source = read_model()?;
    let current_model = parse(&current_source)?;
    if super::is_v1_source(&current_source) {
        reject_v1_options(&args, component_kind)?;
        return run_v1(
            args,
            invocation,
            (component_kind.label(), legacy_kind),
            stem,
            variants,
            model_path,
            current_source,
            current_model,
        );
    }
    reject_unsupported_options(&args)?;
    let Some(kind) = legacy_kind else {
        return Err(Failure::Told(format!(
            "canonical component `{}` requires `jdl 1`.\n       fix: add the JDL v1 header or migrate the model before generating it",
            component_kind.label()
        )));
    };
    let label = java_to_label(&stem);
    let unit_id = UnitId::parse(format!(
        "unit_{}_{}",
        component_kind.label().replace('-', "_"),
        label
    ))
    .map_err(|error| Failure::Told(format!("could not assign source-unit identity: {error}")))?;
    let kind = (component_kind.label(), kind);
    let declaration = declaration(kind.0, &stem, &variants, unit_id.as_str(), &args);
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
                authored_migration: None,
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
                authored_migration: None,
            });
        }
        return Err(Failure::Told(format!(
            "canonical {} `{}` is already declared with a different shape.\n       fix: remove it explicitly before changing its package or kind",
            kind.0, args.name
        )));
    }
    let next_source = append_declaration(current_source.clone(), &declaration)?;
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
        authored_migration: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_v1(
    args: GenerateArgs,
    invocation: Invocation,
    kind: (&str, Option<UnitKind>),
    stem: String,
    variants: Vec<String>,
    model_path: PathBuf,
    current_source: String,
    current_model: jails_model::AppModel,
) -> Result<()> {
    if args.package.is_some() {
        return Err(Failure::Told(format!(
            "JDL v1 derives the managed destination for component {} `{stem}`.\n       fix: remove `--package`; eject its implementation boundary for a reader-owned destination",
            kind.0
        )));
    }
    if args.consumes.is_some() && args.path.is_none() {
        return Err(Failure::Told(
            "a JDL controller can override its wire format only with an explicit route.\n       fix: add `--path <route>` or remove `--consumes`"
                .to_string(),
        ));
    }
    let label = crate::model_resource::java_to_label(&stem);
    let component_id = ComponentId::parse(format!("cmp_{}_{}", kind.0.replace('-', "_"), label))
        .map_err(Failure::Told)?;
    let unit_id = kind
        .1
        .map(|_| UnitId::parse(component_id.to_string()).map_err(Failure::Told))
        .transpose()?;
    let declaration = v1_declaration(
        kind.0,
        &stem,
        &variants,
        component_id.as_str(),
        &args,
        &current_model,
    )?;
    let next_source = if current_model.components.contains_key(&component_id) {
        replace_v1_declaration(&current_source, &stem, &declaration)?
    } else {
        append_declaration(current_source.clone(), &declaration)?
    };
    let next_model = parse(&next_source)?;
    let component = next_model
        .components
        .get(&component_id)
        .cloned()
        .ok_or_else(|| Failure::Told(format!("new component `{component_id}` did not link")))?;
    let unit = unit_id
        .as_ref()
        .map(|unit_id| {
            next_model.units.get(unit_id).cloned().ok_or_else(|| {
                Failure::Told(format!("component `{component_id}` has no emitter view"))
            })
        })
        .transpose()?;
    let existing = current_model.components.get(&component_id);
    if existing == Some(&component)
        && unit_id
            .as_ref()
            .is_none_or(|unit_id| current_model.units.get(unit_id) == unit.as_ref())
    {
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
    let mut patches = if existing.is_some() {
        vec![ModelPatch::ReplaceComponent(component.clone())]
    } else {
        vec![ModelPatch::AddComponent(component.clone())]
    };
    if let Some(unit) = unit.clone() {
        patches.push(if existing.is_some() {
            ModelPatch::ReplaceUnit(unit)
        } else {
            ModelPatch::AddUnit(unit)
        });
    }
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": if existing.is_some() { "replace-component" } else { "add-component" },
        "component": component,
        "unit_view": unit,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: args.name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch: ModelPatch::Batch(patches),
        patch_bytes,
        authored_migration: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use jails_model::ComponentKind;
    use std::collections::BTreeSet;

    #[test]
    fn familiar_component_frontend_covers_the_closed_v1_registry_exactly() {
        let artifact_kinds = [
            ArtifactKind::Class,
            ArtifactKind::Interface,
            ArtifactKind::Service,
            ArtifactKind::Controller,
            ArtifactKind::Sealed,
            ArtifactKind::Strategy,
            ArtifactKind::Handler,
            ArtifactKind::Command,
            ArtifactKind::Cli,
            ArtifactKind::Cases,
            ArtifactKind::Client,
            ArtifactKind::Fetcher,
            ArtifactKind::Job,
            ArtifactKind::HttpWorkflow,
            ArtifactKind::HttpSink,
            ArtifactKind::Idempotency,
            ArtifactKind::Auth,
            ArtifactKind::Webhook,
            ArtifactKind::DurableJob,
            ArtifactKind::Socket,
            ArtifactKind::Presence,
            ArtifactKind::Test,
            ArtifactKind::IntegrationTest,
        ];
        let routed = artifact_kinds
            .into_iter()
            .map(|kind| component_kind(kind).expect("component artifact must be routed"))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            routed,
            ComponentKind::ALL.into_iter().collect(),
            "CLI and JDL component registries diverged"
        );
    }

    #[test]
    fn cases_component_identity_is_derived_from_its_reader_source() {
        assert_eq!(
            component_stem(ArtifactKind::Cases, "specs/01-normalise.md").unwrap(),
            "Case01Normalise"
        );
    }
}
