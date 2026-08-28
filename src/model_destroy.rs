//! Canonical deletion is model subtraction followed by ordinary compilation.

use crate::Invocation;
use crate::cli::StoragePolicy;
use crate::generate::ArtifactKind;
use crate::model_generate::{PreparedMutation, finish_generation};
use crate::model_resource::java_to_label;
use jails_model::{Facet, ModelPatch, OperationKind, StableId, StorageRetirementPolicy, UnitKind};
use jails_support::{Failure, Result};
use serde_json::json;
use std::path::PathBuf;

pub(crate) struct Request {
    pub(crate) kind: ArtifactKind,
    pub(crate) name: String,
    pub(crate) package: bool,
    pub(crate) storage: Option<StoragePolicy>,
    pub(crate) confirm_table: Option<String>,
    pub(crate) migration_effect: bool,
}

pub(crate) fn owns() -> bool {
    crate::model_command::owns()
}

pub(crate) fn run(request: Request, invocation: Invocation) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    if request.package || request.migration_effect {
        return Err(Failure::Told(
            "canonical removal does not accept legacy path or migration-effect flags.\n       fix: remove those flags; managed output and storage retirement are one exact model plan"
                .to_string(),
        ));
    }

    let model_path = model_path(jdl);
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })?;
    let current_model = parse_model(&current_source, jdl)?;
    let label = java_to_label(&request.name);
    let (patch, next_source, patch_bytes) = if matches!(
        request.kind,
        ArtifactKind::Record | ArtifactKind::Value | ArtifactKind::Enum | ArtifactKind::Scaffold
    ) {
        let entity = current_model
            .entities
            .values()
            .find(|entity| entity.label == label || entity.names.java_type == request.name)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "canonical entity `{}` does not exist.\n       fix: name an entity declared under `[entities]`",
                    request.name
                ))
            })?;
        let id = entity.id.clone();
        let stored = current_model
            .capabilities
            .values()
            .any(|capability| capability.kind == "db")
            && entity.facets.contains(&Facet::Repository);
        let (patch, next, kind) = if stored {
            match (request.storage, request.confirm_table.as_deref()) {
                (None, _) => {
                    return Err(Failure::Told(format!(
                        "storage-policy-required: `{}` is backed by table `{}`.\n       fix: preserve it with `jails destroy scaffold {} --storage preserve`, or plan data loss with `--storage drop --confirm-table {}`",
                        entity.names.java_type,
                        entity.names.sql_table,
                        entity.names.java_type,
                        entity.names.sql_table
                    )));
                }
                (Some(StoragePolicy::Preserve), Some(_)) => {
                    return Err(Failure::Told(
                        "`--confirm-table` is only meaningful with `--storage drop`.\n       fix: remove the confirmation when preserving storage"
                            .to_string(),
                    ));
                }
                (Some(StoragePolicy::Preserve), None) => (
                    ModelPatch::RetireEntity {
                        entity: id.clone(),
                        policy: StorageRetirementPolicy::Preserve,
                    },
                    if jdl {
                        crate::model_generate_jdl::set_entity_active(
                            &current_source,
                            &entity.names.java_type,
                            false,
                        )?
                    } else {
                        jails_model::set_entity_active(&current_source, &entity.label, false)
                            .map_err(Failure::Told)?
                    },
                    "retire-entity-preserve-storage",
                ),
                (Some(StoragePolicy::Drop), None) => {
                    return Err(Failure::Told(format!(
                        "dropping `{}` needs its exact table confirmation.\n       fix: pass `--storage drop --confirm-table {}`",
                        entity.names.java_type, entity.names.sql_table
                    )));
                }
                (Some(StoragePolicy::Drop), Some(confirmed)) => (
                    ModelPatch::RetireEntity {
                        entity: id.clone(),
                        policy: StorageRetirementPolicy::Drop {
                            confirmed_table: confirmed.to_string(),
                        },
                    },
                    if jdl {
                        crate::model_generate_jdl::remove_entity(
                            &current_source,
                            &entity.names.java_type,
                            entity.id.as_str(),
                        )?
                    } else {
                        jails_model::remove_entity_declaration(&current_source, &entity.label)
                            .map_err(Failure::Told)?
                    },
                    "retire-entity-drop-storage",
                ),
            }
        } else {
            if request.storage.is_some() || request.confirm_table.is_some() {
                return Err(Failure::Told(format!(
                    "`{}` has no accepted table to retire.\n       fix: remove the storage flags",
                    entity.names.java_type
                )));
            }
            (
                ModelPatch::RemoveEntity(id.clone()),
                if jdl {
                    crate::model_generate_jdl::remove_entity(
                        &current_source,
                        &entity.names.java_type,
                        entity.id.as_str(),
                    )?
                } else {
                    jails_model::remove_entity_declaration(&current_source, &entity.label)
                        .map_err(Failure::Told)?
                },
                "remove-entity",
            )
        };
        let mut proof = current_model.clone();
        proof.apply(patch.clone()).map_err(Failure::Told)?;
        let bytes = serde_json::to_vec(&json!({
            "kind": kind,
            "entity": id,
            "storage": match &patch {
                ModelPatch::RetireEntity { policy, .. } => Some(format!("{policy:?}")),
                _ => None,
            },
        }))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
        (patch, next, bytes)
    } else if matches!(
        request.kind,
        ArtifactKind::Factory | ArtifactKind::Dto | ArtifactKind::Repo
    ) {
        if !jdl {
            return Err(Failure::Told(
                "entity facet removal requires the canonical JDL source\n       fix: migrate `.jails/model.toml` to `.jails/model.jdl`, then retry"
                    .to_string(),
            ));
        }
        if request.storage.is_some() || request.confirm_table.is_some() {
            return Err(Failure::Told(
                "this projection facet has no independent storage to retire.\n       fix: remove the storage flags"
                    .to_string(),
            ));
        }
        let entity = current_model
            .entities
            .values()
            .find(|entity| {
                (entity.label == label || entity.names.java_type == request.name)
                    && match request.kind {
                        ArtifactKind::Factory => entity.facets.contains(&Facet::Factory),
                        ArtifactKind::Dto => entity.facets.contains(&Facet::Dto),
                        ArtifactKind::Repo => entity.facets.contains(&Facet::Repository),
                        _ => false,
                    }
            })
            .ok_or_else(|| {
                let name = match request.kind {
                    ArtifactKind::Factory => "factory",
                    ArtifactKind::Dto => "dto",
                    ArtifactKind::Repo => "repository",
                    _ => unreachable!(),
                };
                Failure::Told(format!(
                    "canonical {name} `{}` does not exist.\n       fix: name a record carrying the {name} facet",
                    request.name,
                ))
            })?;
        let id = entity.id.clone();
        let (facet, marker, name) = match request.kind {
            ArtifactKind::Factory => (Facet::Factory, "@factory", "factory"),
            ArtifactKind::Dto => (Facet::Dto, "@dto", "dto"),
            ArtifactKind::Repo => (Facet::Repository, "@repository", "repository"),
            _ => unreachable!(),
        };
        let next = crate::model_generate_jdl::facet::set_marker(
            &current_source,
            &entity.names.java_type,
            marker,
            false,
        )?;
        let next_model = parse_model(&next, true)?;
        if next_model
            .entity(&id)
            .is_some_and(|entity| entity.facets.contains(&facet))
        {
            return Err(Failure::Told(format!(
                "the `{name}` facet is implied by another entity profile.\n       fix: change that profile explicitly instead of destroying one implied facet"
            )));
        }
        let patch = ModelPatch::RemoveFacet {
            entity: id.clone(),
            facet,
        };
        let mut proof = current_model.clone();
        proof.apply(patch.clone()).map_err(Failure::Told)?;
        let bytes = serde_json::to_vec(&json!({
            "kind": "remove-facet",
            "entity": id,
            "facet": name,
        }))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
        (patch, next, bytes)
    } else if jdl
        && crate::model_generate_jdl::is_v1_source(&current_source)
        && crate::model_generate_jdl::component_kind(request.kind).is_some()
    {
        if request.storage.is_some() || request.confirm_table.is_some() {
            return Err(Failure::Told(
                "components have no independent storage to retire.\n       fix: remove the storage flags"
                    .to_string(),
            ));
        }
        let kind = crate::model_generate_jdl::component_kind(request.kind)
            .expect("the component branch checked this kind");
        let stem = crate::model_generate_jdl::component_stem(request.kind, &request.name)?;
        let component = current_model
            .components
            .values()
            .find(|component| {
                component.kind == kind
                    && if request.kind == ArtifactKind::Cases {
                        component.source.as_deref() == Some(request.name.as_str())
                            || component.name == stem
                    } else {
                        component.label == label
                            || component.name == request.name
                            || component.name == stem
                    }
            })
            .ok_or_else(|| {
                Failure::Told(format!(
                    "canonical component {} `{}` does not exist.\n       fix: name a matching component declaration",
                    kind.label(), request.name
                ))
            })?;
        let id = component.id.clone();
        let unit = current_model
            .units
            .values()
            .find(|unit| unit.id.as_str() == id.as_str())
            .map(|unit| unit.id.clone());
        let next = crate::model_generate_jdl::remove_unit(
            &current_source,
            kind.label(),
            &component.name,
            id.as_str(),
        )?;
        let mut patches = vec![ModelPatch::RemoveComponent(id.clone())];
        if let Some(unit) = unit.clone() {
            patches.push(ModelPatch::RemoveUnit(unit));
        }
        let patch = ModelPatch::Batch(patches);
        let mut proof = current_model.clone();
        proof.apply(patch.clone()).map_err(Failure::Told)?;
        let bytes = serde_json::to_vec(&json!({
            "kind": "remove-component",
            "component": id,
            "unit_view": unit,
        }))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
        (patch, next, bytes)
    } else if matches!(
        request.kind,
        ArtifactKind::Class
            | ArtifactKind::Interface
            | ArtifactKind::Service
            | ArtifactKind::Test
            | ArtifactKind::IntegrationTest
            | ArtifactKind::Sealed
            | ArtifactKind::Strategy
            | ArtifactKind::Controller
    ) {
        if request.storage.is_some() || request.confirm_table.is_some() {
            return Err(Failure::Told(
                "source units have no storage to retire.\n       fix: remove the storage flags"
                    .to_string(),
            ));
        }
        let kind = match request.kind {
            ArtifactKind::Class => ("class", UnitKind::Class),
            ArtifactKind::Interface => ("interface", UnitKind::Interface),
            ArtifactKind::Service => ("service", UnitKind::Service),
            ArtifactKind::Test => ("test", UnitKind::Test),
            ArtifactKind::IntegrationTest => ("integration-test", UnitKind::IntegrationTest),
            ArtifactKind::Sealed => ("sealed", UnitKind::Sealed),
            ArtifactKind::Strategy => ("strategy", UnitKind::Strategy),
            ArtifactKind::Controller => ("controller", UnitKind::Controller),
            _ => unreachable!(),
        };
        let stem = jails_generate::generate::strip_redundant_suffix(request.kind, &request.name);
        let label = java_to_label(&stem);
        let unit = current_model
            .units
            .values()
            .find(|unit| {
                unit.kind == kind.1
                    && (unit.label == label
                        || unit.java_type == request.name
                        || unit.java_type == stem)
            })
            .ok_or_else(|| {
                Failure::Told(format!(
                    "canonical {} `{}` does not exist.\n       fix: name a matching declaration in the application model",
                    kind.0, request.name
                ))
            })?;
        let id = unit.id.clone();
        let component = jdl
            .then(|| {
                current_model
                    .components
                    .values()
                    .find(|component| component.id.as_str() == id.as_str())
                    .map(|component| component.id.clone())
            })
            .flatten();
        let next = if jdl {
            crate::model_generate_jdl::remove_unit(&current_source, kind.0, &stem, id.as_str())?
        } else {
            jails_model::remove_unit_declaration(&current_source, &unit.label)
                .map_err(Failure::Told)?
        };
        let bytes = serde_json::to_vec(&json!({
            "kind": "remove-source-unit",
            "unit": id,
            "component": component,
        }))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
        let patch = component.map_or_else(
            || ModelPatch::RemoveUnit(id.clone()),
            |component| {
                ModelPatch::Batch(vec![
                    ModelPatch::RemoveComponent(component),
                    ModelPatch::RemoveUnit(id.clone()),
                ])
            },
        );
        (patch, next, bytes)
    } else if operation_kind(request.kind).is_some() {
        let operation = current_model
            .operations
            .values()
            .find(|operation| {
                (operation.label == label || operation.names.java_type == request.name)
                    && operation_matches(request.kind, &operation.kind)
            })
            .ok_or_else(|| {
                Failure::Told(format!(
                    "canonical {} operation `{}` does not exist.\n       fix: name a matching operation declared under `[operations]`",
                    operation_kind(request.kind).expect("operation kind was checked"),
                    request.name
                ))
            })?;
        let id = operation.id.clone();
        let mut proof = current_model.clone();
        proof
            .apply(ModelPatch::RemoveOperation(id.clone()))
            .map_err(Failure::Told)?;
        let next = if jdl {
            crate::model_generate_jdl::remove_operation(
                &current_source,
                &operation.names.java_type,
                operation.id.as_str(),
            )?
        } else {
            jails_model::remove_operation_declaration(&current_source, &operation.label)
                .map_err(Failure::Told)?
        };
        let bytes = serde_json::to_vec(&json!({
            "kind": "remove-operation",
            "operation": id,
        }))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
        (ModelPatch::RemoveOperation(id), next, bytes)
    } else {
        return Err(Failure::Told(format!(
            "canonical destroy does not map `{}` to a semantic declaration.\n       fix: destroy `record`, `value`, `enum`, `sealed`, `strategy`, `controller`, `scaffold`, `factory`, `dto`, `repo`, `class`, `interface`, `service`, `test`, `integration-test`, `usecase`, `query`, `transition`, or `event`, or edit the application model",
            kind_name(request.kind)
        )));
    };
    parse_model(&next_source, jdl)?;
    finish_generation(PreparedMutation {
        name: request.name,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch,
        patch_bytes,
    })
}

pub(crate) fn revive(selector: String, table: String, invocation: Invocation) -> Result<()> {
    let jdl = crate::model_command::owns_jdl();
    let model_path = model_path(jdl);
    let current_source = std::fs::read_to_string(&model_path).map_err(|error| {
        Failure::Told(format!(
            "could not read canonical model `{}`: {error}",
            model_path.display()
        ))
    })?;
    let current_model = parse_model(&current_source, jdl)?;
    let label = java_to_label(&selector);
    let entity = current_model
        .entities
        .values()
        .find(|entity| entity.label == label || entity.names.java_type == selector)
        .ok_or_else(|| {
            Failure::Told(format!(
                "canonical preserved entity `{selector}` does not exist.\n       fix: name an entity retired with `--storage preserve`"
            ))
        })?;
    let id = entity.id.clone();
    let entity_label = entity.label.clone();
    let next_source = if jdl {
        crate::model_generate_jdl::set_entity_active(
            &current_source,
            &entity.names.java_type,
            true,
        )?
    } else {
        jails_model::set_entity_active(&current_source, &entity_label, true)
            .map_err(Failure::Told)?
    };
    let patch = ModelPatch::ReviveEntity {
        entity: id.clone(),
        confirmed_table: table.clone(),
    };
    let mut proof = current_model.clone();
    proof.apply(patch.clone()).map_err(Failure::Told)?;
    parse_model(&next_source, jdl)?;
    let patch_bytes = serde_json::to_vec(&json!({
        "kind": "revive-entity-preserved-storage",
        "entity": id,
        "confirmed_table": table,
    }))
    .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    finish_generation(PreparedMutation {
        name: selector,
        invocation,
        model_path,
        current_source,
        current_model,
        next_source,
        patch,
        patch_bytes,
    })
}

fn model_path(jdl: bool) -> PathBuf {
    PathBuf::from(if jdl {
        crate::model_command::JDL_PATH
    } else {
        crate::model_command::TOML_PATH
    })
}

fn parse_model(source: &str, jdl: bool) -> Result<jails_model::AppModel> {
    let parsed = if jdl {
        jails_model::parse_jdl(source)
    } else {
        jails_model::parse_toml(source)
    };
    parsed.map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))
}

fn operation_kind(kind: ArtifactKind) -> Option<&'static str> {
    match kind {
        ArtifactKind::Usecase => Some("command"),
        ArtifactKind::Query => Some("query"),
        ArtifactKind::Transition => Some("transition"),
        ArtifactKind::Event => Some("event"),
        _ => None,
    }
}

fn operation_matches(kind: ArtifactKind, operation: &OperationKind) -> bool {
    matches!(
        (kind, operation),
        (ArtifactKind::Usecase, OperationKind::Command(_))
            | (ArtifactKind::Query, OperationKind::Query(_))
            | (ArtifactKind::Transition, OperationKind::Transition(_))
            | (ArtifactKind::Event, OperationKind::Event(_))
    )
}

fn kind_name(kind: ArtifactKind) -> String {
    use clap::ValueEnum as _;
    kind.to_possible_value()
        .map(|value| value.get_name().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
