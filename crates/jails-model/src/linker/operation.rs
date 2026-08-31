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
    derive_missing_routes(&mut operations, routes, linker);
    operations
}

/// The HTTP route an operation gets when its declaration names none.
///
/// **Without this the `api` capability serves whichever operations happened to
/// spell a path.** Replaying the minicom manifest canonically emitted two
/// controllers where the legacy engine emitted six, because `emit_http` returns
/// `None` for an operation with no route and only two of the six declared one.
/// The legacy engine derived `/actions/send-message` and
/// `/queries/conversation` for the rest, so moving engines silently withdrew
/// four endpoints -- and nothing failed, because a controller only one side
/// writes is not a difference any differential suite can see.
///
/// Derived rather than required, because a route is a *name* and `jdl-sol.md`
/// §7.2 puts every derived name in `AppModel.derived` with the rule that
/// produced it. `derived.rs` already reads this field and already sets
/// `pinned` from whether the author spelled one, so a convention that moves
/// cannot move silently and `jails model explain` shows which routes are the
/// compiler's.
///
/// **`internal` is what says "no endpoint", and this is what makes it mean
/// something.** The flag has been parsed off `@internal` since the grammar had
/// it and no emitter read it; an operation that must not be exposed said so
/// and was exposed anyway the moment a route appeared. It is the guard here.
///
/// Both spellings are set. The flat `route: String` is what `emit_http` reads
/// and the rich `semantics.route` is what `derived.rs` reads, so setting one
/// gives either a controller nobody recorded or a record of a controller
/// nobody wrote.
///
/// The prefixes are the legacy engine's, so a project moving between them
/// keeps its URLs. A transition takes `/{id}` on the end because the canonical
/// transition controller binds `@PathVariable("id")` and refuses a route
/// without it -- the legacy shape carried the key in the body instead.
fn derive_missing_routes(
    operations: &mut BTreeMap<OperationId, Operation>,
    routes: &mut BTreeMap<String, String>,
    linker: &mut Linker,
) {
    for operation in operations.values_mut() {
        let path = format!("$.operations.{}", operation.label);
        if crate::operation::declared_route(&operation.kind).is_some() {
            continue;
        }
        let Some(route) = crate::operation::conventional_route(&operation.label, &operation.kind)
        else {
            continue;
        };
        let (method, route_path) = (route.method, route.path.clone());
        let spelled = crate::operation::spell_route(&route);
        // Registered in the same map the declared routes went into, so a
        // derived route landing on a declared one is the ordinary collision
        // rather than two controllers claiming one path at runtime.
        if let Some(first) = routes.insert(spelled.clone(), path.clone()) {
            linker.problem(
                "model-route-collision",
                format!("{path}.route"),
                format!("derived HTTP route `{spelled}` is already declared at {first}"),
                "give one of them an explicit `route`, or mark this operation `internal`",
            );
            continue;
        }
        let route = crate::operation::OperationRoute {
            method,
            path: route_path,
            consumes: None,
        };
        let flat = spelled;
        match &mut operation.kind {
            OperationKind::Command(spec) => {
                spec.route = Some(flat);
                spec.semantics.route = Some(route);
            }
            OperationKind::Query(spec) => {
                spec.route = Some(flat);
                spec.semantics.route = Some(route);
            }
            OperationKind::Transition(spec) => {
                spec.route = Some(flat);
                spec.semantics.route = Some(route);
            }
            OperationKind::Event(_) => {}
        }
    }
}

mod kinds;
use kinds::*;
