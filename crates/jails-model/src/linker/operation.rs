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
                    linker.route(route.as_deref(), &path, routes);
                    let mut semantics = link_query_semantics(
                        semantics,
                        &path,
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
                        semantics,
                        &path,
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
    let semantics = linked::CommandSemantics {
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
    };
    validate_http_semantics(
        RoutedKind::Command,
        &semantics.parameters,
        &semantics.bindings,
        semantics.route.as_ref(),
        semantics.internal,
        path,
        linker,
    );
    let parameter_names = semantics
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    for resolution in &semantics.resolutions {
        if !parameter_names.contains(resolution.parameter.as_str()) {
            linker.problem(
                "model-operation-resolution-parameter",
                format!("{path}.semantics.resolutions"),
                format!(
                    "resolve references undeclared parameter `{}`",
                    resolution.parameter
                ),
                "use a declared operation parameter",
            );
        }
    }
    semantics
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
    let semantics = linked::QuerySemantics {
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
    };
    validate_http_semantics(
        RoutedKind::Query,
        &semantics.parameters,
        &semantics.bindings,
        semantics.route.as_ref(),
        semantics.internal,
        path,
        linker,
    );
    semantics
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
    let semantics = linked::TransitionSemantics {
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
    };
    validate_http_semantics(
        RoutedKind::Transition,
        &semantics.parameters,
        &semantics.bindings,
        semantics.route.as_ref(),
        semantics.internal,
        path,
        linker,
    );
    validate_transition_roles(&semantics, path, linker);
    semantics
}

#[derive(Clone, Copy)]
enum RoutedKind {
    Command,
    Query,
    Transition,
}

#[allow(clippy::too_many_arguments)]
fn validate_http_semantics(
    kind: RoutedKind,
    parameters: &[linked::OperationParameter],
    bindings: &[linked::ParameterBinding],
    route: Option<&linked::OperationRoute>,
    internal: bool,
    path: &str,
    linker: &mut Linker,
) {
    if internal && (route.is_some() || !bindings.is_empty()) {
        linker.problem(
            "model-operation-internal-http",
            format!("{path}.semantics"),
            "an internal operation cannot declare a route or request bindings",
            "remove route/bind statements or remove `@internal`",
        );
    }
    let parameter_names = parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut bound = BTreeSet::new();
    for binding in bindings {
        if !parameter_names.contains(binding.parameter.as_str()) {
            linker.problem(
                "model-operation-binding-parameter",
                format!("{path}.semantics.bindings"),
                format!(
                    "binding references undeclared parameter `{}`",
                    binding.parameter
                ),
                "bind a declared operation parameter",
            );
        }
        if !bound.insert(binding.parameter.as_str()) {
            linker.problem(
                "model-operation-binding-collision",
                format!("{path}.semantics.bindings"),
                format!("parameter `{}` is bound more than once", binding.parameter),
                "keep one binding source per parameter",
            );
        }
    }
    let Some(route) = route else {
        return;
    };
    let method_allowed = match kind {
        RoutedKind::Command => route.method == crate::EndpointMethod::Post,
        RoutedKind::Query => matches!(
            route.method,
            crate::EndpointMethod::Get | crate::EndpointMethod::Post
        ),
        RoutedKind::Transition => matches!(
            route.method,
            crate::EndpointMethod::Put | crate::EndpointMethod::Patch | crate::EndpointMethod::Post
        ),
    };
    if !method_allowed {
        linker.problem(
            "model-operation-route-method",
            format!("{path}.semantics.route"),
            "the explicit HTTP method is not valid for this operation kind",
            "use a method from the JDL operation route registry",
        );
    }
    if matches!(
        route.method,
        crate::EndpointMethod::Get | crate::EndpointMethod::Delete
    ) && route.consumes == Some(crate::RequestFormat::Json)
    {
        linker.problem(
            "model-operation-route-body",
            format!("{path}.semantics.route"),
            "GET and DELETE routes cannot consume a JSON body in JDL v1",
            "use query/form binding or choose a body-carrying method",
        );
    }
}

fn validate_transition_roles(
    transition: &linked::TransitionSemantics,
    path: &str,
    linker: &mut Linker,
) {
    let select = transition.select.iter().collect::<BTreeSet<_>>();
    let update = transition.update.iter().collect::<BTreeSet<_>>();
    if let Some(field) = select.intersection(&update).next() {
        linker.problem(
            "model-transition-field-role",
            format!("{path}.semantics"),
            format!("field `{field}` appears in both select and update"),
            "give every transition parameter exactly one role",
        );
    }
    for assignment in &transition.assignments {
        if select.contains(&assignment.field) || update.contains(&assignment.field) {
            linker.problem(
                "model-transition-field-role",
                format!("{path}.semantics.assignments"),
                format!(
                    "constant field `{}` also appears in select or update",
                    assignment.field
                ),
                "remove the duplicate transition role",
            );
        }
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
