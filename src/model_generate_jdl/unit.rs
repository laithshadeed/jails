//! Familiar standalone main/test CLI syntax over one source-unit node.

use super::component::{
    component_kind, component_stem, legacy_unit_kind, reject_v1_options, replace_v1_declaration,
    v1_declaration,
};
use super::{MODEL_PATH, append_declaration, parse, read_model};
use crate::ArtifactKind;
use crate::Invocation;
use crate::cli::GenerateArgs;
use crate::model_generate::{PreparedMutation, finish_generation};
use jails_model::{ComponentId, ModelPatch, StableId, UnitId, UnitKind};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(super) fn run(mut args: GenerateArgs, invocation: Invocation) -> Result<()> {
    // The same rule entities get: a name typed in lower camel case is the Java
    // type it names, which is what the legacy path writes and what a later
    // `result:Outcome` has to resolve against. See
    // `model_generate_jdl::java_type_name`.
    //
    // **Except `cases`, whose name is a file rather than a class.** It is one
    // of the two kinds `CLAUDE.md` names for that -- `jails g cases brief.md`
    // reads the reader's brief, and capitalising it asks for `Brief.md`, a
    // file nobody has.
    if args.kind != ArtifactKind::Cases {
        args.name = super::java_type_name(&args.name);
    }
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
    let current_source = read_model(&invocation)?;
    let current_model = parse(&current_source)?;
    reject_v1_options(&args, component_kind)?;
    run_v1(
        args,
        invocation,
        (component_kind.label(), legacy_kind),
        stem,
        variants,
        model_path,
        current_source,
        current_model,
    )
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
