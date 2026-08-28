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

struct EventRegistry<'a> {
    operation_ids: &'a BTreeMap<String, OperationId>,
    event_labels: &'a BTreeSet<String>,
}

pub(super) fn link(
    source_operations: BTreeMap<String, source::Operation>,
    entities: &BTreeMap<EntityId, Entity>,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
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
    let mut routes = BTreeMap::<String, String>::new();
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
                    linker.route(route.as_deref(), &path, &mut routes);
                    let semantics = link_command_semantics(
                        semantics,
                        &path,
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
                    linker.route(route.as_deref(), &path, &mut routes);
                    let semantics = link_query_semantics(
                        semantics,
                        &path,
                        &entity,
                        entity_labels,
                        entity_fields,
                        linker,
                    );
                    OperationKind::Query(Query {
                        on: entity,
                        filters,
                        order_by,
                        limit,
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
                    linker.route(route.as_deref(), &path, &mut routes);
                    let semantics = link_transition_semantics(
                        semantics,
                        &path,
                        &entity,
                        entity_labels,
                        entity_fields,
                        &events,
                        linker,
                    );
                    OperationKind::Transition(Transition {
                        on: entity,
                        fields,
                        sets,
                        yields,
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
                let semantics = link_event_semantics(
                    semantics,
                    &path,
                    entity.as_ref(),
                    entity_labels,
                    entity_fields,
                    linker,
                );
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

fn link_command_semantics(
    source: source::CommandSemantics,
    path: &str,
    entity: &EntityId,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    events: &EventRegistry<'_>,
    linker: &mut Linker,
) -> linked::CommandSemantics {
    linked::CommandSemantics {
        parameters: link_parameters(
            source.parameters,
            path,
            Some(entity),
            entity_labels,
            entity_fields,
            &BTreeMap::new(),
            false,
            linker,
        ),
        assignments: link_assignments(source.assignments, path, entity, entity_fields, linker),
        resolutions: source
            .resolutions
            .into_iter()
            .filter_map(|resolution| {
                link_resolution(
                    resolution,
                    path,
                    entity,
                    entity_labels,
                    entity_fields,
                    linker,
                )
            })
            .collect(),
        conflict_key: link_local_fields(
            &source.conflict_key,
            &format!("{path}.semantics.conflict_key"),
            entity,
            entity_fields,
            linker,
        ),
        emits: link_emits(
            source.emits,
            path,
            events.operation_ids,
            events.event_labels,
            linker,
        ),
        bindings: source.bindings.into_iter().map(link_binding).collect(),
        route: source.route.map(link_route),
        internal: source.internal,
    }
}

fn link_query_semantics(
    source: source::QuerySemantics,
    path: &str,
    entity: &EntityId,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    linker: &mut Linker,
) -> linked::QuerySemantics {
    let mut aliases = BTreeMap::new();
    let joins = source
        .joins
        .into_iter()
        .filter_map(|join| {
            link_join(
                join,
                path,
                entity,
                entity_labels,
                entity_fields,
                &mut aliases,
                linker,
            )
        })
        .collect();
    linked::QuerySemantics {
        parameters: link_parameters(
            source.parameters,
            path,
            Some(entity),
            entity_labels,
            entity_fields,
            &aliases,
            true,
            linker,
        ),
        joins,
        order: source
            .order
            .into_iter()
            .filter_map(|ordering| {
                let field = link_visible_field(
                    &ordering.field,
                    &format!("{path}.semantics.order"),
                    Some(entity),
                    entity_labels,
                    entity_fields,
                    &aliases,
                    linker,
                )?;
                Some(linked::Ordering {
                    field,
                    direction: match ordering.direction {
                        source::SortDirection::Asc => linked::SortDirection::Asc,
                        source::SortDirection::Desc => linked::SortDirection::Desc,
                    },
                })
            })
            .collect(),
        limit: source.limit,
        bindings: source.bindings.into_iter().map(link_binding).collect(),
        route: source.route.map(link_route),
        internal: source.internal,
    }
}

fn link_transition_semantics(
    source: source::TransitionSemantics,
    path: &str,
    entity: &EntityId,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    events: &EventRegistry<'_>,
    linker: &mut Linker,
) -> linked::TransitionSemantics {
    linked::TransitionSemantics {
        parameters: link_parameters(
            source.parameters,
            path,
            Some(entity),
            entity_labels,
            entity_fields,
            &BTreeMap::new(),
            false,
            linker,
        ),
        select: link_local_fields(
            &source.select,
            &format!("{path}.semantics.select"),
            entity,
            entity_fields,
            linker,
        ),
        update: link_local_fields(
            &source.update,
            &format!("{path}.semantics.update"),
            entity,
            entity_fields,
            linker,
        ),
        assignments: link_assignments(source.assignments, path, entity, entity_fields, linker),
        precondition: source.precondition.map(|precondition| match precondition {
            source::Precondition::Required => linked::Precondition::Required,
            source::Precondition::Optional => linked::Precondition::Optional,
            source::Precondition::None => linked::Precondition::None,
        }),
        emits: link_emits(
            source.emits,
            path,
            events.operation_ids,
            events.event_labels,
            linker,
        ),
        bindings: source.bindings.into_iter().map(link_binding).collect(),
        route: source.route.map(link_route),
        internal: source.internal,
    }
}

fn link_event_semantics(
    source: source::EventSemantics,
    path: &str,
    entity: Option<&EntityId>,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    linker: &mut Linker,
) -> linked::EventSemantics {
    let parameters = link_parameters(
        source.parameters,
        path,
        entity,
        entity_labels,
        entity_fields,
        &BTreeMap::new(),
        false,
        linker,
    );
    if let Some(partition) = &source.partition_by
        && !parameters
            .iter()
            .any(|parameter| &parameter.name == partition)
    {
        linker.problem(
            "model-event-partition",
            format!("{path}.semantics.partition_by"),
            format!("`{partition}` does not name an event parameter"),
            "partition by a declared payload parameter",
        );
    }
    linked::EventSemantics {
        parameters,
        partition_by: source.partition_by,
    }
}
