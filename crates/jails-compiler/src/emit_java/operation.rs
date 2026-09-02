//! An operation's port as a recipe row: the interface a command, query or
//! transition is called through, and the record an event is.
//!
//! Four rows over one [`Recipe<Operation>`], one per operation kind, each
//! selected by `only_when`. What differs between the kinds is spelled by
//! the template; what a template cannot say is a fragment here: the `ROUTE`
//! constant, the answer type, the `ExecutionContext` a scoped entity's
//! operations take, the row selector a transition keys on, the expected
//! version an `if-match` transition takes, and the `Input` record -- which
//! is the same `record_declarations` and `record_constructor` every record
//! renders through, bound by the one [`Binder`] decision, so the request
//! binding stays decided once.
//!
//! The event's record reuses the entity record's template: a record is a
//! record, and only what fills `{{components}}` differs.

use super::*;
use crate::recipe::{Fragment, Import, JavaFile, Naming, Placement, Recipe, Rendered, SourceSet};

/// The ports, one row per kind.
pub(super) const PORTS: Recipe<Operation> = Recipe {
    substitutions: &[],
    keys: &[],
    fragments: &[
        Fragment::Rendered {
            key: "route",
            render: route,
        },
        Fragment::Rendered {
            key: "answer",
            render: answer,
        },
        Fragment::Rendered {
            key: "result",
            render: result,
        },
        Fragment::Rendered {
            key: "entity",
            render: entity_type,
        },
        Fragment::Rendered {
            key: "context",
            render: context,
        },
        Fragment::Rendered {
            key: "limit",
            render: limit,
        },
        Fragment::Rendered {
            key: "key",
            render: key,
        },
        Fragment::Rendered {
            key: "expected",
            render: expected,
        },
        Fragment::Rendered {
            key: "input",
            render: input,
        },
        Fragment::Rendered {
            key: "components",
            render: event_components,
        },
        Fragment::Rendered {
            key: "compact_constructor",
            render: event_constructor,
        },
    ],
    requires: &[],
    files: &[
        port(
            "command",
            crate::template!("spring/operation_command_java.java"),
            Package::ApplicationCommands,
            command_class,
            is_command,
        ),
        port(
            "query",
            crate::template!("spring/operation_query_java.java"),
            Package::ApplicationQueries,
            query_class,
            is_query,
        ),
        port(
            "transition",
            crate::template!("spring/operation_transition_java.java"),
            Package::ApplicationTransitions,
            transition_class,
            is_transition,
        ),
        port(
            "event",
            crate::template!("spring/entity_record_java.java"),
            Package::DomainEvents,
            event_class,
            is_event,
        ),
    ],
    files_when: crate::recipe::BootCondition::Any,
    resources: &[],
    dependencies: &[],
    properties: &[],
    compose_services: &[],
    build_features: &[],
    default_package: application_package,
    pass: "java-operations",
    minimum_boot: None,
};

const fn port(
    role: &'static str,
    template: crate::Template,
    layer: Package,
    class: fn(&Operation) -> String,
    only_when: fn(&AppModel, &Operation) -> bool,
) -> JavaFile<Operation> {
    JavaFile {
        role,
        template,
        before_boot: None,
        imports: &[] as &[Import<Operation>],
        only_when: Some(only_when),
        source_set: SourceSet::Main,
        placement: Placement::Layer(layer),
        // A port is managed ABI: its adapters and its proof name it.
        ejectable: false,
        class: Naming::By(class),
        template_class: Naming::By(class),
    }
}

fn application_package(model: &AppModel, _: &Operation) -> String {
    model.project.package_for(Package::Application)
}

fn command_class(operation: &Operation) -> String {
    with_suffix(&operation.names.java_type, "Command")
}

fn query_class(operation: &Operation) -> String {
    with_suffix(&operation.names.java_type, "Query")
}

fn transition_class(operation: &Operation) -> String {
    with_suffix(&operation.names.java_type, "Transition")
}

fn event_class(operation: &Operation) -> String {
    with_suffix(&operation.names.java_type, "Event")
}

fn is_command(_: &AppModel, operation: &Operation) -> bool {
    matches!(operation.kind, OperationKind::Command(_))
}

fn is_query(_: &AppModel, operation: &Operation) -> bool {
    matches!(operation.kind, OperationKind::Query(_))
}

fn is_transition(_: &AppModel, operation: &Operation) -> bool {
    matches!(operation.kind, OperationKind::Transition(_))
}

fn is_event(_: &AppModel, operation: &Operation) -> bool {
    matches!(operation.kind, OperationKind::Event(_))
}

/// The entity a command, query or transition is on.
fn target<'a>(model: &'a AppModel, operation: &Operation) -> Result<&'a Entity, CompileError> {
    let id = match &operation.kind {
        OperationKind::Command(command) => &command.on,
        OperationKind::Query(query) => &query.on,
        OperationKind::Transition(transition) => &transition.on,
        OperationKind::Event(_) => unreachable!("an event's record names no target entity"),
    };
    entity(model, id)
}

/// `String ROUTE = "...";`, for a routed operation; nothing otherwise.
fn route(_: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let route = match &operation.kind {
        OperationKind::Command(command) => command.route.as_deref(),
        OperationKind::Query(query) => query.route.as_deref(),
        OperationKind::Transition(transition) => transition.route.as_deref(),
        OperationKind::Event(_) => None,
    };
    Ok(Rendered::from(route.map_or_else(String::new, |route| {
        format!("    String ROUTE = {};\n\n", java_string(route))
    })))
}

/// What a command answers with.
///
/// **A resolved key can miss, and that is an outcome rather than a fault.**
/// The insert selects the foreign key out of the parent's own row, so a
/// caller naming a parent that is not there writes nothing -- which is a 404,
/// not a 500.
fn answer(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let OperationKind::Command(command) = &operation.kind else {
        unreachable!("only a command spells {{answer}}");
    };
    let entity = target(model, operation)?;
    let mut imports = BTreeSet::from([domain_import(model, entity)]);
    let text = if command.semantics.resolutions.is_empty() {
        entity.names.java_type.clone()
    } else {
        imports.insert("java.util.Optional".to_string());
        format!("Optional<{}>", entity.names.java_type)
    };
    Ok(Rendered { text, imports })
}

/// What a query answers with: a list of the entity.
fn result(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let entity = target(model, operation)?;
    Ok(Rendered {
        text: format!("List<{}>", entity.names.java_type),
        imports: BTreeSet::from(["java.util.List".to_string(), domain_import(model, entity)]),
    })
}

/// The entity's type, imported from its domain package.
fn entity_type(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let entity = target(model, operation)?;
    Ok(Rendered {
        text: entity.names.java_type.clone(),
        imports: BTreeSet::from([domain_import(model, entity)]),
    })
}

/// `ExecutionContext context, ` when the entity has a scoped field, so the
/// port takes the request boundary its adapter proves the claim against.
fn context(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let entity = target(model, operation)?;
    let mut imports = BTreeSet::new();
    Ok(Rendered {
        text: operation_context(model, entity, &mut imports),
        imports,
    })
}

/// A query's `DEFAULT_LIMIT`, when it declares one.
fn limit(_: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let OperationKind::Query(query) = &operation.kind else {
        unreachable!("only a query spells {{limit}}");
    };
    Ok(Rendered::from(
        query.semantics.limit.map_or_else(String::new, |limit| {
            format!("    int DEFAULT_LIMIT = {limit};\n\n")
        }),
    ))
}

/// The row a transition selects, as `execute`'s first argument.
fn key(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let OperationKind::Transition(transition) = &operation.kind else {
        unreachable!("only a transition spells {{key}}");
    };
    let entity = target(model, operation)?;
    let key = transition_key(entity, transition)?;
    let mut imports = BTreeSet::new();
    let key_type = java_type(key, &mut imports);
    Ok(Rendered {
        text: format!("{key_type} {}", key.names.java_member),
        imports,
    })
}

/// The version an `if-match` transition takes, with its leading comma. See
/// [`super::precondition`].
fn expected(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let OperationKind::Transition(transition) = &operation.kind else {
        unreachable!("only a transition spells {{expected}}");
    };
    let entity = target(model, operation)?;
    let mut imports = BTreeSet::new();
    let text = precondition(entity, transition)
        .map(|precondition| precondition.parameter(&mut imports))
        .unwrap_or_default();
    Ok(Rendered { text, imports })
}

/// The `Input` record nested in the port, indented into it.
///
/// **Only a form-bound route needs the binding annotation.** A JSON body
/// reaches Jackson, which applies the project's naming strategy itself; a
/// form reaches Spring's data binder, which has none.
fn input(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let binder = operation
        .route()
        .is_some_and(|route| route.consumes == Some(jails_model::RequestFormat::Form))
        .then(|| Binder {
            model,
            declared: operation.bindings(),
        });
    let mut imports = BTreeSet::new();
    let components = input_components(model, operation, &mut imports)?;
    let text = indent(
        &record_shape_bound("Input", &components, &mut imports, binder),
        4,
    );
    Ok(Rendered { text, imports })
}

/// An event's payload, in order.
///
/// **The linked parameters, not the flat `fields`.** The flat list can only
/// name fields of the target entity, so an event declaring a component the
/// row does not carry -- its own minted `id`, the moment it happened -- would
/// render a record without it, and the emitter that stages the payload would
/// then name an accessor no record has. The linker folds `fields` into the
/// parameters, so this is the whole payload either way; the command's
/// `Input` reads it the same way.
fn event_payload<'a>(
    model: &'a AppModel,
    operation: &'a Operation,
    imports: &mut BTreeSet<String>,
) -> Result<Vec<RecordComponent<'a>>, CompileError> {
    let OperationKind::Event(event) = &operation.kind else {
        unreachable!("only an event spells its record's components");
    };
    if !event.semantics.parameters.is_empty() {
        return parameter_components(model, &event.semantics.parameters, imports);
    }
    let Some(entity_id) = event.on.as_ref() else {
        return Ok(Vec::new());
    };
    Ok(input::field_components(
        fields(entity(model, entity_id)?, &event.fields)?.into_iter(),
    ))
}

fn event_components(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let mut imports = BTreeSet::new();
    let components = event_payload(model, operation, &mut imports)?;
    let declarations = input::record_declarations(&components, &mut imports, None);
    let text = match components.is_empty() {
        true => String::new(),
        false => format!("\n{declarations}\n"),
    };
    Ok(Rendered { text, imports })
}

fn event_constructor(model: &AppModel, operation: &Operation) -> Result<Rendered, CompileError> {
    let mut imports = BTreeSet::new();
    let components = event_payload(model, operation, &mut imports)?;
    let text = input::record_constructor("{{class}}", &components, &mut imports);
    Ok(Rendered { text, imports })
}
