//! The four operation kinds, their parameters, and the statement vocabulary
//! each one accepts.
//!
//! **`explicit` is the rule worth finding here.** The linker derives a route
//! for an operation under an `http` projection and leaves the compatibility
//! `route` -- what the source said -- empty. Re-emitting the derived one turns
//! a convention into a declaration.

use super::{field_label, json, type_token, value};
use crate::EntityId;
use crate::id::StableId;
use crate::model::AppModel;
use crate::operation::{
    BindingSource, Delivery, OperationKind, ParameterSource, Precondition, SortDirection,
};
use std::fmt::Write as _;

pub(super) fn owner(kind: &OperationKind) -> Option<&EntityId> {
    match kind {
        OperationKind::Command(command) => Some(&command.on),
        OperationKind::Query(query) => Some(&query.on),
        OperationKind::Transition(transition) => Some(&transition.on),
        OperationKind::Event(event) => event.on.as_ref(),
    }
}

pub(super) fn write_top_level_operations(model: &AppModel, out: &mut String) {
    for operation in model.operations.values() {
        if owner(&operation.kind).is_none() {
            write_operation(model, operation, out, "");
            out.push('\n');
        }
    }
}

pub(super) fn write_operation(
    model: &AppModel,
    operation: &crate::Operation,
    out: &mut String,
    indent: &str,
) {
    let (keyword, parameters, internal) = match &operation.kind {
        OperationKind::Command(command) => (
            "command",
            &command.semantics.parameters,
            command.semantics.internal,
        ),
        OperationKind::Query(query) => (
            "query",
            &query.semantics.parameters,
            query.semantics.internal,
        ),
        OperationKind::Transition(transition) => (
            "transition",
            &transition.semantics.parameters,
            transition.semantics.internal,
        ),
        OperationKind::Event(event) => ("event", &event.semantics.parameters, false),
    };
    let rendered = parameters
        .iter()
        .map(|parameter| render_parameter(model, parameter))
        .collect::<Vec<_>>()
        .join(", ");
    let flag = if internal { " @internal" } else { "" };
    let _ = writeln!(
        out,
        "{indent}{keyword} {}({rendered}) @id({}){flag} {{",
        operation.names.java_type,
        operation.id.as_str()
    );
    let body = format!("{indent}  ");
    match &operation.kind {
        OperationKind::Command(command) => {
            let on = &command.on;
            if command.semantics.delivery != Delivery::default() {
                let _ = writeln!(out, "{body}deliver outbox");
            }
            for assignment in &command.semantics.assignments {
                let _ = writeln!(
                    out,
                    "{body}set {} = {}",
                    field_label(model, on, &assignment.field),
                    value(&assignment.value)
                );
            }
            if !command.semantics.conflict_key.is_empty() {
                let fields = command
                    .semantics
                    .conflict_key
                    .iter()
                    .map(|field| field_label(model, on, field).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(out, "{body}conflict on [{fields}]");
            }
            for resolution in &command.semantics.resolutions {
                let remote = model
                    .entities
                    .get(&resolution.remote_entity)
                    .map(|entity| entity.names.java_type.as_str())
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "{body}resolve {} from {remote}.{} where {remote}.{} = {}",
                    field_label(model, on, &resolution.target),
                    field_label(model, &resolution.remote_entity, &resolution.remote_value),
                    field_label(model, &resolution.remote_entity, &resolution.remote_lookup),
                    resolution.parameter
                );
            }
            write_route(
                out,
                &body,
                explicit(command.route.as_ref(), command.semantics.route.as_ref()),
            );
            write_bindings(out, &body, &command.semantics.bindings);
            write_emits(model, out, &body, &command.semantics.emits);
        }
        OperationKind::Query(query) => {
            for join in &query.semantics.joins {
                let joined = model
                    .entities
                    .get(&join.entity)
                    .map(|entity| entity.names.java_type.as_str())
                    .unwrap_or_default();
                let mappings = join
                    .mappings
                    .iter()
                    .map(|mapping| {
                        format!(
                            "{} -> {}",
                            field_label(model, &query.on, &mapping.local),
                            field_label(model, &join.entity, &mapping.remote)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(out, "{body}join {joined} as {} on {mappings}", join.alias);
            }
            if !query.semantics.order.is_empty() {
                let order = query
                    .semantics
                    .order
                    .iter()
                    .map(|ordering| {
                        let label = visible(model, &ordering.field);
                        match ordering.direction {
                            SortDirection::Desc => format!("{label} desc"),
                            SortDirection::Asc => label,
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(out, "{body}order by [{order}]");
            }
            if let Some(limit) = query.semantics.limit {
                let _ = writeln!(out, "{body}limit {limit}");
            }
            write_route(
                out,
                &body,
                explicit(query.route.as_ref(), query.semantics.route.as_ref()),
            );
            write_bindings(out, &body, &query.semantics.bindings);
        }
        OperationKind::Transition(transition) => {
            let on = &transition.on;
            for assignment in &transition.semantics.assignments {
                let _ = writeln!(
                    out,
                    "{body}set {} = {}",
                    field_label(model, on, &assignment.field),
                    value(&assignment.value)
                );
            }
            if !transition.semantics.select.is_empty() {
                let fields = labels(model, on, &transition.semantics.select);
                let _ = writeln!(out, "{body}select [{fields}]");
            }
            if !transition.semantics.update.is_empty() {
                let fields = labels(model, on, &transition.semantics.update);
                let _ = writeln!(out, "{body}update [{fields}]");
            }
            if let Some(precondition) = &transition.semantics.precondition {
                let policy = match precondition {
                    Precondition::Required => "required",
                    Precondition::Optional => "optional",
                    Precondition::None => "none",
                };
                let _ = writeln!(out, "{body}if-match {policy}");
            }
            write_route(
                out,
                &body,
                explicit(
                    transition.route.as_ref(),
                    transition.semantics.route.as_ref(),
                ),
            );
            write_bindings(out, &body, &transition.semantics.bindings);
            write_emits(model, out, &body, &transition.semantics.emits);
        }
        OperationKind::Event(event) => {
            if let Some(parameter) = &event.semantics.partition_by {
                let _ = writeln!(out, "{body}partition by {parameter}");
            }
        }
    }
    let _ = writeln!(out, "{indent}}}");
}

fn labels(model: &AppModel, entity: &EntityId, fields: &[crate::FieldId]) -> String {
    fields
        .iter()
        .map(|field| field_label(model, entity, field).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn visible(model: &AppModel, field: &crate::operation::VisibleField) -> String {
    let label = field_label(model, &field.entity, &field.field).to_string();
    match &field.qualifier {
        Some(qualifier) => format!("{qualifier}.{label}"),
        None => label,
    }
}

fn render_parameter(model: &AppModel, parameter: &crate::operation::OperationParameter) -> String {
    match &parameter.source {
        ParameterSource::Field(field) => {
            let path = visible(model, field);
            let optional = if parameter.optional_filter { "?" } else { "" };
            let default = field_label(model, &field.entity, &field.field);
            if parameter.name == default && field.qualifier.is_none() {
                format!("{path}{optional}")
            } else {
                format!("{path}{optional} as {}", parameter.name)
            }
        }
        ParameterSource::Typed(ty) => {
            let mut rendered = format!(
                "{}: {}{}",
                parameter.name,
                type_token(ty),
                if parameter.required { "" } else { "?" }
            );
            rendered.push_str(&constraint_attributes(&parameter.constraints));
            rendered
        }
    }
}

/// The route only when the author wrote one.
///
/// **The linker derives a route for an operation under an `http` projection**,
/// and puts it in `semantics.route` while leaving the compatibility `route`
/// -- which records what the *source* said -- empty. Re-emitting the derived
/// one turns a convention into a declaration: the next `model explain` stops
/// reporting it as derived, and a convention that moves can no longer move.
/// Same rule as `FieldDefault::derived`, arriving from the other side.
fn explicit<'a>(
    written: Option<&String>,
    effective: Option<&'a crate::operation::OperationRoute>,
) -> Option<&'a crate::operation::OperationRoute> {
    written.and(effective)
}

/// The attributes a parameter carries, wherever it appears.
///
/// **Shared between operations and components on purpose.** They were two
/// copies, and the component one had only `@default` -- so `name: string
/// @notBlank` on a `durable-job` lost its check, silently, in the one
/// direction where losing it rewrites somebody's model. Found by the round
/// trip over the golden corpus rather than by reading either copy.
pub(super) fn constraint_attributes(
    constraints: &crate::operation::ParameterConstraints,
) -> String {
    let mut rendered = String::new();
    if let Some(default) = &constraints.default {
        let _ = write!(rendered, " @default({})", value(default));
    }
    if let Some(length) = &constraints.length {
        let min = length.min.map(|min| min.to_string()).unwrap_or_default();
        let max = length.max.map(|max| max.to_string()).unwrap_or_default();
        let _ = write!(rendered, " @length({min}..{max})");
    }
    if constraints.nonnegative {
        rendered.push_str(" @nonnegative");
    }
    if constraints.non_blank {
        rendered.push_str(" @notBlank");
    }
    if constraints.positive {
        rendered.push_str(" @positive");
    }
    rendered
}

pub(super) fn write_route(
    out: &mut String,
    body: &str,
    route: Option<&crate::operation::OperationRoute>,
) {
    let Some(route) = route else {
        return;
    };
    let consumes = route
        .consumes
        .map(|format| {
            let word = match format {
                crate::RequestFormat::Json => "json",
                crate::RequestFormat::Form => "form",
            };
            format!(" consumes {word}")
        })
        .unwrap_or_default();
    let _ = writeln!(
        out,
        "{body}route {} {}{consumes}",
        method(route.method),
        json(&route.path)
    );
}

fn method(method: crate::EndpointMethod) -> &'static str {
    match method {
        crate::EndpointMethod::Get => "GET",
        crate::EndpointMethod::Post => "POST",
        crate::EndpointMethod::Put => "PUT",
        crate::EndpointMethod::Patch => "PATCH",
        crate::EndpointMethod::Delete => "DELETE",
    }
}

pub(super) fn write_bindings(
    out: &mut String,
    body: &str,
    bindings: &[crate::operation::ParameterBinding],
) {
    for binding in bindings {
        let source = match binding.source {
            BindingSource::Path => "path",
            BindingSource::Query => "query",
            BindingSource::Header => "header",
            BindingSource::Claim => "claim",
            BindingSource::Form => "form",
        };
        let wire = binding
            .wire_name
            .as_deref()
            .map(|name| format!(" {}", json(name)))
            .unwrap_or_default();
        let _ = writeln!(out, "{body}bind {} from {source}{wire}", binding.parameter);
    }
}

fn write_emits(model: &AppModel, out: &mut String, body: &str, emits: &[crate::OperationId]) {
    for emitted in emits {
        let name = model
            .operations
            .get(emitted)
            .map(|operation| operation.names.java_type.as_str())
            .unwrap_or_default();
        let _ = writeln!(out, "{body}emit {name}");
    }
}
