//! Resolving an operation's parameters, assignments, emissions and route.
//!
//! **This is where "the fact a renderer turns on" is decided, once.** Each
//! `OperationParameter` comes out of here with a `ParameterSource` — the
//! caller's body, the path, the request context, a constant — and its
//! constraints attached. Every emitter and every generated test then reads
//! that resolved answer instead of re-deriving it from the declaration, which
//! is the drift a route renderer and its own test suite once had between them.
//!
//! Every function takes `&mut Linker` and reports rather than returning early,
//! so one pass over a model with several unresolved names produces one
//! diagnostic set. That is why the argument lists are long: the resolution
//! needs the entity labels, the per-entity field maps and the aliases in
//! scope, and threading them beats rebuilding them per parameter.

use super::super::Linker;
use crate::id::{EntityId, FieldId, OperationId};
use crate::operation as linked;
use crate::source;
use std::collections::{BTreeMap, BTreeSet};

#[allow(clippy::too_many_arguments)]
pub(super) fn link_parameters(
    parameters: Vec<source::OperationParameter>,
    path: &str,
    local_entity: Option<&EntityId>,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    aliases: &BTreeMap<String, EntityId>,
    allow_optional_filter: bool,
    linker: &mut Linker,
) -> Vec<linked::OperationParameter> {
    let mut names = BTreeSet::new();
    parameters
        .into_iter()
        .filter_map(|parameter| {
            let parameter_path = format!("{path}.semantics.parameters.{}", parameter.name);
            if !names.insert(parameter.name.clone()) {
                linker.problem(
                    "model-operation-parameter-collision",
                    &parameter_path,
                    format!(
                        "operation parameter `{}` is declared more than once",
                        parameter.name
                    ),
                    "give every operation parameter a unique effective name",
                );
                return None;
            }
            if parameter.optional_filter && !allow_optional_filter {
                linker.problem(
                    "model-operation-optional-parameter",
                    &parameter_path,
                    "presence-sensitive `?` parameters are valid only on queries",
                    "remove `?` or move the filter to a query",
                );
            }
            let source = match parameter.source {
                source::ParameterSource::Field { path: field_path } => {
                    linked::ParameterSource::Field(link_visible_field(
                        &field_path,
                        &parameter_path,
                        local_entity,
                        entity_labels,
                        entity_fields,
                        aliases,
                        linker,
                    )?)
                }
                source::ParameterSource::Typed { type_name } => {
                    match crate::TypeRef::parse(&type_name) {
                        Ok(ty) => linked::ParameterSource::Typed(ty),
                        Err(message) => {
                            linker.problem(
                                "model-operation-parameter-type",
                                &parameter_path,
                                message,
                                "use a builtin type or declared Java type",
                            );
                            return None;
                        }
                    }
                }
            };
            Some(linked::OperationParameter {
                name: parameter.name,
                source,
                required: parameter.required,
                optional_filter: parameter.optional_filter,
                constraints: linked::ParameterConstraints {
                    default: parameter.constraints.default.map(link_value),
                    non_blank: parameter.constraints.non_blank,
                    length: if parameter.constraints.min_length.is_some()
                        || parameter.constraints.max_length.is_some()
                    {
                        Some(crate::LengthRange {
                            min: parameter.constraints.min_length,
                            max: parameter.constraints.max_length,
                        })
                    } else {
                        None
                    },
                    positive: parameter.constraints.positive,
                    nonnegative: parameter.constraints.nonnegative,
                },
            })
        })
        .collect()
}

pub(super) fn link_visible_field(
    path: &str,
    diagnostic_path: &str,
    local_entity: Option<&EntityId>,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    aliases: &BTreeMap<String, EntityId>,
    linker: &mut Linker,
) -> Option<linked::VisibleField> {
    let (qualifier, field_label, entity) = if let Some((qualifier, field)) = path.split_once('.') {
        let entity = aliases
            .get(qualifier)
            .or_else(|| entity_labels.get(qualifier))
            .cloned();
        (Some(qualifier.to_string()), field, entity)
    } else {
        (None, path, local_entity.cloned())
    };
    let Some(entity) = entity else {
        linker.problem(
            "model-operation-field-scope",
            diagnostic_path,
            format!("`{path}` does not name a visible operation field"),
            "use a local field, joined alias, or declared entity qualifier",
        );
        return None;
    };
    let field = link_local_field(field_label, diagnostic_path, &entity, entity_fields, linker)?;
    Some(linked::VisibleField {
        entity,
        field,
        qualifier,
    })
}

fn link_local_field(
    label: &str,
    path: &str,
    entity: &EntityId,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    linker: &mut Linker,
) -> Option<FieldId> {
    linker
        .field_refs(&[label.to_string()], path, entity, entity_fields)
        .into_iter()
        .next()
}

pub(super) fn link_local_fields(
    labels: &[String],
    path: &str,
    entity: &EntityId,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    linker: &mut Linker,
) -> Vec<FieldId> {
    linker.field_refs(labels, path, entity, entity_fields)
}

pub(super) fn link_assignments(
    assignments: Vec<source::Assignment>,
    path: &str,
    entity: &EntityId,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    linker: &mut Linker,
) -> Vec<linked::Assignment> {
    assignments
        .into_iter()
        .filter_map(|assignment| {
            let field = link_local_field(
                &assignment.field,
                &format!("{path}.semantics.assignments"),
                entity,
                entity_fields,
                linker,
            )?;
            Some(linked::Assignment {
                field,
                value: link_value(assignment.value),
            })
        })
        .collect()
}

pub(super) fn link_resolution(
    resolution: source::Resolution,
    path: &str,
    local_entity: &EntityId,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    linker: &mut Linker,
) -> Option<linked::Resolution> {
    let target = link_local_field(
        &resolution.target,
        &format!("{path}.semantics.resolutions"),
        local_entity,
        entity_fields,
        linker,
    )?;
    let remote_value = link_visible_field(
        &resolution.remote_value,
        &format!("{path}.semantics.resolutions"),
        None,
        entity_labels,
        entity_fields,
        &BTreeMap::new(),
        linker,
    )?;
    let remote_lookup = link_visible_field(
        &resolution.remote_lookup,
        &format!("{path}.semantics.resolutions"),
        None,
        entity_labels,
        entity_fields,
        &BTreeMap::new(),
        linker,
    )?;
    if remote_value.entity != remote_lookup.entity {
        linker.problem(
            "model-operation-resolution-entity",
            format!("{path}.semantics.resolutions"),
            "resolve value and lookup fields belong to different entities",
            "resolve and look up fields on the same remote entity",
        );
        return None;
    }
    Some(linked::Resolution {
        target,
        remote_entity: remote_value.entity,
        remote_value: remote_value.field,
        remote_lookup: remote_lookup.field,
        parameter: resolution.parameter,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn link_join(
    join: source::Join,
    path: &str,
    local_entity: &EntityId,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    aliases: &mut BTreeMap<String, EntityId>,
    linker: &mut Linker,
) -> Option<linked::Join> {
    let remote_entity = linker.entity_ref(
        &join.entity,
        &format!("{path}.semantics.joins"),
        entity_labels,
    )?;
    if aliases
        .insert(join.alias.clone(), remote_entity.clone())
        .is_some()
    {
        linker.problem(
            "model-operation-join-alias",
            format!("{path}.semantics.joins"),
            format!("join alias `{}` is declared more than once", join.alias),
            "give every join a unique alias",
        );
        return None;
    }
    let mappings = join
        .mappings
        .into_iter()
        .filter_map(|mapping| {
            let local = link_local_field(
                mapping.local.rsplit('.').next().unwrap_or(&mapping.local),
                &format!("{path}.semantics.joins"),
                local_entity,
                entity_fields,
                linker,
            )?;
            let remote = link_local_field(
                mapping.remote.rsplit('.').next().unwrap_or(&mapping.remote),
                &format!("{path}.semantics.joins"),
                &remote_entity,
                entity_fields,
                linker,
            )?;
            Some(linked::FieldMapping { local, remote })
        })
        .collect();
    Some(linked::Join {
        entity: remote_entity,
        alias: join.alias,
        mappings,
    })
}

pub(super) fn link_emits(
    emits: Vec<String>,
    path: &str,
    operation_ids: &BTreeMap<String, OperationId>,
    operation_is_event: &BTreeSet<String>,
    linker: &mut Linker,
) -> Vec<OperationId> {
    emits
        .into_iter()
        .filter_map(|event| {
            if !operation_is_event.contains(&event) {
                linker.problem(
                    "model-event-reference",
                    format!("{path}.semantics.emits"),
                    format!("`{event}` does not name an event operation"),
                    "emit an operation whose kind is `event`",
                );
                return None;
            }
            operation_ids.get(&event).cloned()
        })
        .collect()
}

pub(super) fn link_route(route: source::OperationRoute) -> linked::OperationRoute {
    linked::OperationRoute {
        method: route.method,
        path: route.path,
        consumes: route.consumes,
    }
}

pub(super) fn link_binding(binding: source::ParameterBinding) -> linked::ParameterBinding {
    linked::ParameterBinding {
        parameter: binding.parameter,
        source: match binding.source {
            source::BindingSource::Path => linked::BindingSource::Path,
            source::BindingSource::Query => linked::BindingSource::Query,
            source::BindingSource::Header => linked::BindingSource::Header,
            source::BindingSource::Claim => linked::BindingSource::Claim,
            source::BindingSource::Form => linked::BindingSource::Form,
        },
        wire_name: binding.wire_name,
    }
}

fn link_value(value: source::Value) -> linked::Value {
    match value {
        source::Value::String(value) => linked::Value::String(value),
        source::Value::Integer(value) => linked::Value::Integer(value),
        source::Value::Decimal(value) => linked::Value::Decimal(value),
        source::Value::Boolean(value) => linked::Value::Boolean(value),
        source::Value::EnumConstant(value) => linked::Value::EnumConstant(value),
        source::Value::Function(call) => linked::Value::Function {
            name: call.name,
            arguments: call.arguments.into_iter().map(link_value).collect(),
        },
    }
}
