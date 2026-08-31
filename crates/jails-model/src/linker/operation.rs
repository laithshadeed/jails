//! Linking the four verbs: resolving every reference an operation names.
//!
//! An authored operation is labels — an entity, some field names, maybe a
//! route. A linked one is IDs, with each parameter carrying where its value
//! comes from and what constrains it. That resolution happens once, here, so
//! no emitter has to do it again and no two emitters can do it differently.
//!
//! Split three ways by what the work is: this module walks the declarations,
//! `semantics.rs` resolves the parts that become `*Semantics`, and
//! `field_rules.rs` holds the cross-cutting checks that need the whole model
//! (which fields an operation may touch given the capabilities present).
//!
//! Errors accumulate into the `Linker` rather than returning early, which is
//! the same rule `Diagnostics` exists for: a model with four unresolved names
//! should report four.

mod field_rules;
mod semantics;

use super::Linker;
use crate::id::{EntityId, FieldId, OperationId};
use crate::model::Entity;
use crate::naming::upper_camel_case;
use crate::operation::{
    self as linked, Command, Event, Operation, OperationKind, OperationNames, Query, Transition,
};
use crate::source;
use semantics::{
    link_assignments, link_binding, link_emits, link_join, link_local_fields, link_parameters,
    link_resolution, link_route, link_visible_field,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_field_rules(
    operations: &BTreeMap<OperationId, Operation>,
    entities: &BTreeMap<EntityId, Entity>,
    capabilities: &BTreeMap<crate::CapabilityId, crate::Capability>,
    linker: &mut Linker,
) {
    field_rules::validate(operations, entities, capabilities, linker);
}

struct EventRegistry<'a> {
    operation_ids: &'a BTreeMap<String, OperationId>,
    event_labels: &'a BTreeSet<String>,
}

pub(super) fn link(
    source_operations: BTreeMap<String, source::Operation>,
    entities: &BTreeMap<EntityId, Entity>,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    routes: &mut BTreeMap<String, String>,
    linker: &mut Linker,
) -> BTreeMap<OperationId, Operation> {
    let mut operation_ids = BTreeMap::new();
    let mut operation_java_types = BTreeMap::new();
    let mut operation_is_event = BTreeSet::new();
    for (label, operation) in &source_operations {
        let path = format!("$.operations.{label}");
        linker.label(label, &path);
        let raw_id = operation.id();
        linker.register_id(raw_id, &format!("{path}.id"));
        if let Some(id) = linker.stable_id::<OperationId>(raw_id, &format!("{path}.id")) {
            operation_ids.insert(label.clone(), id);
        }
        let java_type = operation
            .java_name()
            .map(str::to_string)
            .unwrap_or_else(|| upper_camel_case(label));
        linker.java_type(&java_type, &format!("{path}.java_name"));
        operation_java_types.insert(label.clone(), java_type);
        if matches!(operation, source::Operation::Event { .. }) {
            operation_is_event.insert(label.clone());
        }
    }

    let mut operations = BTreeMap::new();
    let events = EventRegistry {
        operation_ids: &operation_ids,
        event_labels: &operation_is_event,
    };
    for (label, operation) in source_operations {
        let path = format!("$.operations.{label}");
        let Some(id) = operation_ids.get(&label).cloned() else {
            continue;
        };
        let kind = match operation {
            source::Operation::Command {
                on,
                fields,
                route,
                semantics,
                ..
            } => linker
                .entity_ref(&on, &format!("{path}.on"), entity_labels)
                .map(|entity| {
                    let fields = linker.field_refs(
                        &fields,
                        &format!("{path}.fields"),
                        &entity,
                        entity_fields,
                    );
                    linker.route(route.as_deref(), &path, routes);
                    let semantics = link_command_semantics(
                        Declaration {
                            label: &label,
                            path: &path,
                            route: route.as_deref(),
                        },
                        semantics,
                        &entity,
                        entity_labels,
                        entity_fields,
                        &events,
                        linker,
                    );
                    OperationKind::Command(Command {
                        on: entity,
                        fields,
                        route,
                        semantics,
                    })
                }),
            source::Operation::Query {
                on,
                filters,
                order_by,
                limit,
                route,
                semantics,
                ..
            } => linker
                .entity_ref(&on, &format!("{path}.on"), entity_labels)
                .map(|entity| {
                    let filters = linker.field_refs(
                        &filters,
                        &format!("{path}.filters"),
                        &entity,
                        entity_fields,
                    );
                    let order_by = linker.field_refs(
                        &order_by,
                        &format!("{path}.order_by"),
                        &entity,
                        entity_fields,
                    );
                    if limit == Some(0) {
                        linker.problem(
                            "model-query-limit",
                            format!("{path}.limit"),
                            "a query limit cannot be zero",
                            "remove `limit` or use a positive number",
                        );
                    }
                    linker.route(route.as_deref(), &path, routes);
                    let mut semantics = link_query_semantics(
                        Declaration {
                            label: &label,
                            path: &path,
                            route: route.as_deref(),
                        },
                        semantics,
                        &entity,
                        entity_labels,
                        entity_fields,
                        linker,
                    );
                    // `.jails/model.toml` spells only the flat `order_by`
                    // list, which has nowhere to put a direction, so it folds
                    // in as ascending. A source that spells the rich form
                    // wins.
                    if semantics.order.is_empty() {
                        semantics.order = order_by
                            .into_iter()
                            .map(|field| linked::Ordering {
                                field: linked::VisibleField {
                                    entity: entity.clone(),
                                    field,
                                    qualifier: None,
                                },
                                direction: linked::SortDirection::Asc,
                            })
                            .collect();
                    }
                    if semantics.limit.is_none() {
                        semantics.limit = limit;
                    }
                    OperationKind::Query(Query {
                        on: entity,
                        filters,
                        route,
                        semantics,
                    })
                }),
            source::Operation::Transition {
                on,
                fields,
                sets,
                yields,
                route,
                semantics,
                ..
            } => linker
                .entity_ref(&on, &format!("{path}.on"), entity_labels)
                .map(|entity| {
                    let fields = linker.field_refs(
                        &fields,
                        &format!("{path}.fields"),
                        &entity,
                        entity_fields,
                    );
                    let sets =
                        linker.field_refs(&sets, &format!("{path}.sets"), &entity, entity_fields);
                    let yields = yields.and_then(|target| {
                        if !operation_is_event.contains(&target) {
                            linker.problem(
                                "model-event-reference",
                                format!("{path}.yields"),
                                format!("`{target}` does not name an event operation"),
                                "name an operation whose kind is `event`",
                            );
                            return None;
                        }
                        operation_ids.get(&target).cloned()
                    });
                    linker.route(route.as_deref(), &path, routes);
                    let mut semantics = link_transition_semantics(
                        Declaration {
                            label: &label,
                            path: &path,
                            route: route.as_deref(),
                        },
                        semantics,
                        &entity,
                        entity_labels,
                        entity_fields,
                        &events,
                        linker,
                    );
                    // `.jails/model.toml` spells only the flat `sets`/`yields`
                    // pair, so it is folded into the rich fields here and the
                    // linked transition keeps one home per fact. A source that
                    // spells the rich form wins: JDL v1 fills both, and its
                    // `sets` projection is the one that was wrong.
                    if semantics.update.is_empty() {
                        semantics.update = sets;
                    }
                    if semantics.emits.is_empty() {
                        semantics.emits.extend(yields);
                    }
                    OperationKind::Transition(Transition {
                        on: entity,
                        fields,
                        route,
                        semantics,
                    })
                }),
            source::Operation::Event {
                on,
                fields,
                semantics,
                ..
            } => {
                let entity = on.as_deref().and_then(|label| {
                    linker.entity_ref(label, &format!("{path}.on"), entity_labels)
                });
                if entity.is_none() && !fields.is_empty() {
                    linker.problem(
                        "model-event-fields",
                        format!("{path}.fields"),
                        "an event without `on` cannot reference entity fields",
                        "set `on` to an entity label or remove `fields`",
                    );
                }
                let fields = entity.as_ref().map_or_else(Vec::new, |entity| {
                    linker.field_refs(&fields, &format!("{path}.fields"), entity, entity_fields)
                });
                let mut semantics = link_event_semantics(
                    semantics,
                    &path,
                    entity.as_ref(),
                    entity_labels,
                    entity_fields,
                    linker,
                );
                // **The flat spelling folds into the rich one, here.** `fields`
                // is `.jails/model.toml`'s and the pre-v1 draft's way of saying
                // what an event carries, and it can only name fields of the
                // target entity. `semantics.parameters` can also carry a
                // `Typed` component -- an event's own identity, a timestamp --
                // which the flat form cannot express at all.
                //
                // Emitters read the rich form only, so the two cannot disagree
                // about a payload. This is the last of `audit.md` A3.9: the
                // same fold transitions and queries already got, and the one
                // that was still losing data, because an emitter reading
                // `fields` renders an empty payload for an event declared with
                // typed components.
                if semantics.parameters.is_empty() {
                    semantics.parameters = fields
                        .iter()
                        .filter_map(|field| {
                            let entity = entity.clone()?;
                            let name = entity_fields
                                .get(&entity)?
                                .iter()
                                .find(|(_, id)| *id == field)
                                .map(|(name, _)| name.clone())?;
                            Some(crate::operation::OperationParameter {
                                name,
                                source: crate::operation::ParameterSource::Field(
                                    crate::operation::VisibleField {
                                        entity,
                                        field: field.clone(),
                                        qualifier: None,
                                    },
                                ),
                                required: true,
                                optional_filter: false,
                                constraints: crate::operation::ParameterConstraints::default(),
                            })
                        })
                        .collect();
                }
                Some(OperationKind::Event(Event {
                    on: entity,
                    fields,
                    semantics,
                }))
            }
        };
        if let Some(kind) = kind {
            if crate::operation::entity(&kind)
                .and_then(|entity| entities.get(entity))
                .is_some_and(|entity| !entity.active)
            {
                linker.problem(
                    "model-retired-entity-reference",
                    format!("{path}.on"),
                    "an operation cannot target a retired entity",
                    "revive the entity or remove the operation",
                );
                continue;
            }
            // A derived route claims its path exactly as a declared one does.
            // `linker.route` above sees only what the author wrote, so without
            // this a convention could quietly take a path some other operation
            // had pinned, and the collision would surface as two Spring
            // handlers mapped to one URL -- a context that fails to start,
            // named after a route nobody typed.
            if let (None, Some(derived)) = crate::operation::routes(&kind) {
                linker.route(Some(&derived.canonical()), &path, routes);
            }
            let java_type = operation_java_types
                .remove(&label)
                .expect("every operation label receives a Java projection");
            operations.insert(
                id.clone(),
                Operation {
                    id,
                    label,
                    names: OperationNames { java_type },
                    kind,
                },
            );
        }
    }
    operations
}

mod kinds;
use kinds::*;
