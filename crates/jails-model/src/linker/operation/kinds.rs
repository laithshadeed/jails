//! What one operation kind means, per kind.
//!
//! Split from `operation.rs` by the boundary that file already had, made
//! explicit. `semantics.rs` beside this resolves a *reference* -- a label to
//! an entity, a name to a field, a string to a route. This says what a
//! command, query, transition or event does with the references it was given.
//! The two change for different reasons: a new reference form touches every
//! kind, and a new kind touches none of the resolvers.

use super::*;

/// Which operation is being linked: its label, and the diagnostic path built
/// from it.
///
/// One value rather than two parameters because they are always passed
/// together and always derived from each other -- and because splitting `path`
/// back apart to recover `label` is the re-derivation this crate spends its
/// doc comments warning about.
#[derive(Clone, Copy)]
pub(super) struct Declaration<'a> {
    pub(super) label: &'a str,
    pub(super) path: &'a str,
    /// The flat `METHOD /path` spelling, which is the *only* place a
    /// `.jails/model.toml` project states its route.
    pub(super) route: Option<&'a str>,
}

/// The delivery policy, or a diagnostic naming the two that exist.
///
/// A command that delivers through an outbox and emits nothing is refused
/// separately, in `validate`: the policy is about *how* events travel, so one
/// with no events is a declaration that does nothing.
fn link_delivery(delivery: Option<&str>, path: &str, linker: &mut Linker) -> linked::Delivery {
    match delivery {
        None | Some("direct") => linked::Delivery::Direct,
        Some("outbox") => linked::Delivery::Outbox,
        Some(other) => {
            linker.problem(
                "model-command-delivery",
                format!("{path}.semantics.delivery"),
                format!("`{other}` is not a delivery policy"),
                "use `direct` or `outbox`",
            );
            linked::Delivery::Direct
        }
    }
}

pub(super) fn link_command_semantics(
    declaration: Declaration<'_>,
    source: source::CommandSemantics,
    entity: &EntityId,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    events: &EventRegistry<'_>,
    linker: &mut Linker,
) -> linked::CommandSemantics {
    let Declaration { label, path, route } = declaration;
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
        delivery: link_delivery(source.delivery.as_deref(), path, linker),
        bindings: source.bindings.into_iter().map(link_binding).collect(),
        // A route the author did not declare is derived rather than
        // absent -- see `derived_route`. `internal` is the one shape that
        // stays off HTTP, because saying so is what `@internal` means.
        route: source
            .route
            .map(link_route)
            .or_else(|| flat_route(route))
            .or_else(|| (!source.internal).then(|| derived_route(RoutedKind::Command, label))),
        internal: source.internal,
    };
    // A policy about *how* events travel, on a command with none, is a
    // declaration that does nothing -- and it would emit an outbox table,
    // store, worker and relay that never carry a row.
    if semantics.delivery == linked::Delivery::Outbox && semantics.emits.is_empty() {
        linker.problem(
            "model-command-delivery-without-events",
            format!("{path}.semantics.delivery"),
            "`outbox` delivery needs at least one event to deliver",
            "add an `emit`, or use the default direct delivery",
        );
    }
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

pub(super) fn link_query_semantics(
    declaration: Declaration<'_>,
    source: source::QuerySemantics,
    entity: &EntityId,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    linker: &mut Linker,
) -> linked::QuerySemantics {
    let Declaration { label, path, route } = declaration;
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
        // A route the author did not declare is derived rather than
        // absent -- see `derived_route`. `internal` is the one shape that
        // stays off HTTP, because saying so is what `@internal` means.
        route: source
            .route
            .map(link_route)
            .or_else(|| flat_route(route))
            .or_else(|| (!source.internal).then(|| derived_route(RoutedKind::Query, label))),
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

pub(super) fn link_transition_semantics(
    declaration: Declaration<'_>,
    source: source::TransitionSemantics,
    entity: &EntityId,
    entity_labels: &BTreeMap<String, EntityId>,
    entity_fields: &BTreeMap<EntityId, BTreeMap<String, FieldId>>,
    events: &EventRegistry<'_>,
    linker: &mut Linker,
) -> linked::TransitionSemantics {
    let Declaration { label, path, route } = declaration;
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
        // A route the author did not declare is derived rather than
        // absent -- see `derived_route`. `internal` is the one shape that
        // stays off HTTP, because saying so is what `@internal` means.
        route: source
            .route
            .map(link_route)
            .or_else(|| flat_route(route))
            .or_else(|| (!source.internal).then(|| derived_route(RoutedKind::Transition, label))),
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

pub(super) fn link_event_semantics(
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

/// The flat `METHOD /path` spelling, as the typed route the rest of the
/// compiler reads.
///
/// **`.jails/model.toml` has no rich route to link**, so this is the whole
/// declaration for a project on the compatibility input -- and reading only
/// the rich one turned a declared `GET /notes/search` into the derived
/// `POST /queries/open-notes`, which is the "flat spelling folds into the rich
/// one, here" rule this linker already applies to `sets`, `yields` and
/// `fields`.
///
/// A malformed string yields `None` rather than a second diagnostic:
/// `Linker::route` has already refused it by name.
fn flat_route(route: Option<&str>) -> Option<linked::OperationRoute> {
    let (method, path) = route?.split_once(' ')?;
    Some(linked::OperationRoute {
        method: crate::EndpointMethod::parse(&method.to_ascii_lowercase()).ok()?,
        path: path.to_string(),
        consumes: None,
    })
}

/// The route an operation answers on when its author declared none.
///
/// **The `api` capability's whole surface used to depend on a declaration.**
/// `emit_http.rs` skips an operation whose `semantics.route` is `None`, so a
/// model that named six operations and pinned two paths got two controllers,
/// while the legacy generator derived the other four. That is not a stricter
/// rule -- it is a silently smaller application, which is the failure mode
/// `derived` exists to make impossible.
///
/// The shape is the legacy engine's, unchanged, so a project that crosses to
/// the compiler keeps the URLs its callers already use: `/actions/<name>` for
/// the two kinds that write and `/queries/<name>` for the one that reads.
///
/// The transition is the one departure, and it is forced rather than chosen.
/// Legacy took the row's key out of the request *body* of a `PUT`, which is a
/// key in two places at once; the canonical controller binds
/// `@PathVariable("id")` and refuses a transition route without `{id}`. So the
/// derived path carries it.
///
/// A route derived here is **not** pinned: `derived::records` reads the
/// author's declaration to decide that, so a convention that moves shows up as
/// a moved convention rather than as a changed contract.
fn derived_route(kind: RoutedKind, label: &str) -> linked::OperationRoute {
    let name = label.replace('_', "-");
    match kind {
        RoutedKind::Command => linked::OperationRoute {
            method: crate::EndpointMethod::Post,
            path: format!("/actions/{name}"),
            consumes: None,
        },
        // **GET, where the legacy engine derived POST.** That engine sent a
        // query's filters as a JSON body, so it needed a verb with one; the
        // canonical controller binds `@ModelAttribute`, which reads the query
        // string and the URI template variables and never a body. A POST here
        // would be a route only a form post could drive.
        RoutedKind::Query => linked::OperationRoute {
            method: crate::EndpointMethod::Get,
            path: format!("/queries/{name}"),
            consumes: None,
        },
        // PUT rather than PATCH for the same reason the legacy recipe chose
        // it: a compare-and-swap update against a version the caller states is
        // idempotent, and PUT is the method that promises that.
        RoutedKind::Transition => linked::OperationRoute {
            method: crate::EndpointMethod::Put,
            path: format!("/actions/{name}/{{id}}"),
            consumes: None,
        },
    }
}
