//! Canonical deletion is model subtraction followed by ordinary compilation.

use crate::ArtifactKind;
use crate::Invocation;
use crate::cli::StoragePolicy;
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

pub(crate) fn run(request: Request, invocation: Invocation) -> Result<()> {
    if request.package || request.migration_effect {
        return Err(Failure::Told(
            "canonical removal does not accept legacy path or migration-effect flags.\n       fix: remove those flags; managed output and storage retirement are one exact model plan"
                .to_string(),
        ));
    }

    let model_path = PathBuf::from(crate::model_command::JDL_PATH);
    let current_source = crate::model_command::read_source(&model_path)?;
    let current_model = crate::model_generate_jdl::parse(&current_source)?;
    let label = java_to_label(&request.name);
    let (patch, next_source, patch_bytes) = if request.kind == ArtifactKind::Association {
        // **Retiring a foreign key is a forward migration, not the un-running
        // of one.** Refusing the verb -- which is where this started -- left
        // both halves of an association permanently undestroyable, so the
        // command exists and names the accepted constraint: the compiler drops
        // exactly `confirmed_name` and refuses a relation that merely stopped
        // being declared.
        if request.storage.is_some() || request.confirm_table.is_some() {
            return Err(Failure::Told(
                "an association has no table of its own to retire.\n       fix: remove the storage flags"
                    .to_string(),
            ));
        }
        let relation = current_model
            .relations
            .values()
            .find(|relation| relation.label == label || relation.sql_name == request.name)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "canonical association `{}` does not exist.\n       fix: name a relation declared on the child entity",
                    request.name
                ))
            })?
            .clone();
        let child = current_model
            .entities
            .get(&relation.child)
            .ok_or_else(|| {
                Failure::Told(format!(
                    "association `{}` names a missing child entity",
                    request.name
                ))
            })?
            .clone();
        // **The member's spelling, not the relation's label.** A relation is
        // declared as a lowerCamel *member* (`relation itemOwner to Owner`)
        // and linked under the stable-fragment label `item_owner`, so removal
        // has to ask the CST for the name it actually wrote.
        let member = crate::model_generate_jdl::relation_member_name(&relation.label);
        let next = jails_model::remove_jdl_entity_member(
            &current_source,
            &child.names.java_type,
            &["relation"],
            Some(&member),
            None,
        )
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
        let patch = ModelPatch::RemoveRelation {
            relation: relation.id.clone(),
            confirmed_name: relation.sql_name.clone(),
        };
        let bytes = serde_json::to_vec(&json!({
            "kind": "remove-relation",
            "relation": relation.id,
            "confirmed_name": relation.sql_name,
        }))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
        (patch, next, bytes)
    } else if matches!(
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
                    crate::model_generate_jdl::set_entity_active(
                        &current_source,
                        &entity.names.java_type,
                        false,
                    )?,
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
                    crate::model_generate_jdl::remove_entity(
                        &current_source,
                        &entity.names.java_type,
                    )?,
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
                crate::model_generate_jdl::remove_entity(&current_source, &entity.names.java_type)?,
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
        ArtifactKind::Factory
            | ArtifactKind::Dto
            | ArtifactKind::Repo
            | ArtifactKind::Search
            | ArtifactKind::Seed
    ) {
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
                        ArtifactKind::Search => entity.facets.contains(&Facet::Search),
                        ArtifactKind::Seed => entity.facets.contains(&Facet::Seed),
                        _ => false,
                    }
            })
            .ok_or_else(|| {
                let name = match request.kind {
                    ArtifactKind::Factory => "factory",
                    ArtifactKind::Dto => "dto",
                    ArtifactKind::Repo => "repository",
                    ArtifactKind::Search => "search",
                    ArtifactKind::Seed => "seed",
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
            // The projection carries its field list, and removal names the
            // projection rather than the fields -- which is why `set_marker`
            // can take it back where `set_projection` had to put it there.
            ArtifactKind::Search => (Facet::Search, "@search", "search"),
            ArtifactKind::Seed => (Facet::Seed, "@seed", "seed"),
            _ => unreachable!(),
        };
        let next = crate::model_generate_jdl::facet::set_marker(
            &current_source,
            &entity.names.java_type,
            marker,
            false,
        )?;
        let next_model = crate::model_generate_jdl::parse(&next)?;
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
    } else if crate::model_generate_jdl::component_kind(request.kind).is_some() {
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
        let next = crate::model_generate_jdl::remove_unit(&current_source, &component.name)?;
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
        let stem = crate::strip_redundant_suffix(request.kind, &request.name);
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
        let component = current_model
            .components
            .values()
            .find(|component| component.id.as_str() == id.as_str())
            .map(|component| component.id.clone());
        let next = crate::model_generate_jdl::remove_unit(&current_source, &stem)?;
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
        let next = crate::model_generate_jdl::remove_operation(
            &current_source,
            &operation.names.java_type,
        )?;
        let bytes = serde_json::to_vec(&json!({
            "kind": "remove-operation",
            "operation": id,
        }))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
        (ModelPatch::RemoveOperation(id), next, bytes)
    } else {
        return Err(Failure::Told(format!(
            "canonical destroy does not map `{}` to a semantic declaration.\n       fix: destroy `record`, `value`, `enum`, `sealed`, `strategy`, `controller`, `scaffold`, `factory`, `dto`, `repo`, `search`, `seed`, `association`, `class`, `interface`, `service`, `test`, `integration-test`, `usecase`, `query`, `transition`, or `event`, or edit the application model",
            kind_name(request.kind)
        )));
    };
    crate::model_generate_jdl::parse(&next_source)?;
    finish_generation(PreparedMutation {
        name: request.name,
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

/// The table this project created and then dropped for a resource it no longer
/// declares, read off the migrations rather than remembered.
fn dropped_table(_source: &str, selector: &str) -> Option<String> {
    let table = jails_model::plural_snake_case(&java_to_label(selector));
    let directory = std::path::Path::new("src/main/resources/db/migration");
    let entries = std::fs::read_dir(directory).ok()?;
    let mut created = false;
    let mut dropped = false;
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        created |= text.contains(&format!("create table {table}"));
        dropped |= text.contains(&format!("drop table {table}"));
    }
    (created && dropped).then_some(table)
}

pub(crate) fn revive(selector: String, table: String, invocation: Invocation) -> Result<()> {
    let model_path = PathBuf::from(crate::model_command::JDL_PATH);
    let current_source = crate::model_command::read_source(&model_path)?;
    let current_model = crate::model_generate_jdl::parse(&current_source)?;
    let label = java_to_label(&selector);
    let entity = current_model
        .entities
        .values()
        .find(|entity| entity.label == label || entity.names.java_type == selector)
        .ok_or_else(|| {
            // **A dropped table is not a preserved one.** `--storage drop`
            // appends a forward migration and takes the declaration with it,
            // so there is nothing to revive: the history says the table was
            // created and dropped, and going back means declaring the resource
            // again and creating a new one.
            if let Some(table) = dropped_table(&current_source, &selector) {
                return Failure::Told(format!(
                    "`{selector}` had an append-only drop planned for `{table}`, so there is no preserved table to revive.\n       fix: declare it again with `jails g scaffold {selector} <field>:<type>`, which creates a new table"
                ));
            }
            Failure::Told(format!(
                "canonical preserved entity `{selector}` does not exist.\n       fix: name an entity retired with `--storage preserve`"
            ))
        })?;
    let id = entity.id.clone();
    let next_source = crate::model_generate_jdl::set_entity_active(
        &current_source,
        &entity.names.java_type,
        true,
    )?;
    let patch = ModelPatch::ReviveEntity {
        entity: id.clone(),
        confirmed_table: table.clone(),
    };
    let mut proof = current_model.clone();
    proof.apply(patch.clone()).map_err(Failure::Told)?;
    crate::model_generate_jdl::parse(&next_source)?;
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
        authored_migration: None,
    })
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
