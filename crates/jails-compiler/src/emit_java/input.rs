//! What an operation's `Input` record declares, and how a request binds to it.
//!
//! **Two readers need this answer and must not differ**: the record renderer
//! builds the components, and the proof renderer builds the request that binds
//! to them. Each working it out for itself is how they drift, so it is one
//! module rather than one convention.

use super::*;

/// What one operation's `Input` record declares, in order.
///
/// **One answer, because two readers need it and they must not differ.** The
/// record renderer builds the components from this, and `emit_http::proof`
/// builds the request that binds to them from the same list. Without it they
/// disagree: a query's `Input` takes entity fields, so its components are
/// `java_member` (`userId`), while a command with linked parameters takes
/// those, so its components are parameter labels (`user_id`). A test that
/// guesses either spelling sends `user_id=1` at a record declaring `userId`
/// and gets a 400.
///
/// The imports the components need are collected here too, for the same
/// reason: the branch that decides the component decides which type it names.
pub(crate) fn input_components<'a>(
    model: &'a AppModel,
    operation: &'a Operation,
    imports: &mut BTreeSet<String>,
) -> Result<Vec<RecordComponent<'a>>, CompileError> {
    let from_fields =
        |entity_id, field_ids: &[jails_model::FieldId], imports: &mut BTreeSet<String>| {
            let entity = entity(model, entity_id)?;
            let fields = fields(entity, field_ids)?;
            import_declared_types(model, &fields, imports);
            Ok::<_, CompileError>(
                fields
                    .into_iter()
                    .map(|field| RecordComponent {
                        name: field.names.java_member.clone(),
                        ty: &field.ty,
                        required: field.required,
                        non_blank: field.non_blank,
                        length: field.length.as_ref(),
                        positive: field.semantics.positive,
                        nonnegative: field.semantics.nonnegative,
                    })
                    .collect::<Vec<_>>(),
            )
        };
    match &operation.kind {
        OperationKind::Command(command) => {
            if command.semantics.parameters.is_empty() {
                from_fields(&command.on, &command.fields, imports)
            } else {
                parameter_components(model, &command.semantics.parameters, imports)
            }
        }
        // **The linked parameters, not the flat `filters`.** A `--via` query
        // filters on a column of the *joined* entity, and the target's own
        // field list cannot hold one -- so reading the flat list emits an
        // `Input` missing that component while the adapter binds it, and the
        // endpoint answers over every row. Same choice the command arm above
        // already makes, and the same shape `Transition`'s own documentation
        // records for `sets`.
        OperationKind::Query(query) => {
            if query.semantics.parameters.is_empty() {
                from_fields(&query.on, &query.filters, imports)
            } else {
                parameter_components(model, &query.semantics.parameters, imports)
            }
        }
        // **Minus the row selector, and minus the version.** `execute` already
        // takes both, and a component bound from two places can disagree with
        // itself -- a `PATCH /conversations/{userId}/status` whose body
        // carries a different `userId` has no honest answer. The version has
        // the same problem and one more: it belongs in `If-Match`, where every
        // cache and client library already knows to put it.
        OperationKind::Transition(transition) => {
            let entity = entity(model, &transition.on)?;
            let key = transition_key(entity, transition)?;
            let expected = precondition(entity, transition).map(|precondition| precondition.field);
            let carried = transition
                .fields
                .iter()
                .filter(|field| *field != &key.id)
                .filter(|field| expected.is_none_or(|version| *field != &version.id))
                .cloned()
                .collect::<Vec<_>>();
            from_fields(&transition.on, &carried, imports)
        }
        OperationKind::Event(_) => Ok(Vec::new()),
    }
}

pub(crate) fn record_shape(
    type_name: &str,
    fields: &[&Field],
    imports: &mut BTreeSet<String>,
) -> String {
    let components = fields
        .iter()
        .map(|field| RecordComponent {
            name: field.names.java_member.clone(),
            ty: &field.ty,
            required: field.required,
            non_blank: field.non_blank,
            length: field.length.as_ref(),
            positive: field.semantics.positive,
            nonnegative: field.semantics.nonnegative,
        })
        .collect::<Vec<_>>();
    record_shape_from_components(type_name, &components, imports)
}

/// Import a project type the model declares, when the record needs it.
///
/// **`java_type_ref` imports an external type only when it is fully
/// qualified**, which a model-declared entity never is -- so an operation
/// `Input` in `application.queries` carrying a `MessageDirection` would
/// compile against a symbol it never imports. The entity's own record gets away with
/// it by living in the same package; nothing above the domain does.
///
/// Called from every shape that can carry one rather than from `record_shape`,
/// which has no model to ask.
pub(crate) fn import_declared_type(model: &AppModel, ty: &TypeRef, imports: &mut BTreeSet<String>) {
    let TypeRef::External(name) = ty else {
        return;
    };
    if name.contains('.') {
        return;
    }
    // **The package the named entity is in, not this one's.** A slice that
    // pinned its own package still refers to types outside it, and importing
    // them from where the *referrer* lives names a class that is not there.
    if let Some(owner) = model
        .entities
        .values()
        .find(|entity| &entity.names.java_type == name)
    {
        imports.insert(format!(
            "{}.{name}",
            entity_package(model, owner, Package::Domain)
        ));
    }
}

/// The same, for every field a record shape is about to render.
fn import_declared_types(model: &AppModel, fields: &[&Field], imports: &mut BTreeSet<String>) {
    for field in fields {
        import_declared_type(model, &field.ty, imports);
    }
}

/// The component names an event's record declares, in order.
///
/// **The same two sources the record body reads**, so a caller asking which
/// component carries an entity's identity cannot get an answer the record
/// disagrees with. `sample` in `emit_component::http_sink` walks the
/// parameters for the same reason.
pub(crate) fn event_component_names(
    model: &AppModel,
    event: &jails_model::Event,
) -> Result<Vec<String>, CompileError> {
    if !event.semantics.parameters.is_empty() {
        return Ok(event
            .semantics
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect());
    }
    let Some(entity_id) = event.on.as_ref() else {
        return Ok(Vec::new());
    };
    let owner = entity(model, entity_id)?;
    Ok(fields(owner, &event.fields)?
        .into_iter()
        .map(|field| field.names.java_member.clone())
        .collect())
}

pub(crate) fn parameter_components<'a>(
    model: &'a AppModel,
    parameters: &'a [OperationParameter],
    imports: &mut BTreeSet<String>,
) -> Result<Vec<RecordComponent<'a>>, CompileError> {
    let components = parameters
        .iter()
        .map(|parameter| {
            let inherited = match &parameter.source {
                ParameterSource::Typed(_) => None,
                ParameterSource::Field(visible) => {
                    let owner = entity(model, &visible.entity)?;
                    let field = owner.field(&visible.field).ok_or_else(|| {
                        CompileError::new(format!(
                            "linked operation parameter `{}` references missing field `{}`",
                            parameter.name, visible.field
                        ))
                    })?;
                    Some(field)
                }
            };
            let (ty, non_blank, length, positive, nonnegative) = if let Some(field) = inherited {
                (
                    &field.ty,
                    field.non_blank,
                    field.length.as_ref(),
                    field.semantics.positive,
                    field.semantics.nonnegative,
                )
            } else {
                let ParameterSource::Typed(ty) = &parameter.source else {
                    unreachable!()
                };
                (
                    ty,
                    parameter.constraints.non_blank,
                    parameter.constraints.length.as_ref(),
                    parameter.constraints.positive,
                    parameter.constraints.nonnegative,
                )
            };
            import_declared_type(model, ty, imports);
            // **camelCase, like every other record component.** A parameter's
            // name is its stable label, which is snake -- so a command's
            // `Input` would ship `user_id` and `is_read` while a query's, built
            // from the field list, ships `userId`. Two operation kinds
            // disagreeing about the JSON wire format of the same column is not
            // a formatting difference.
            let member = parameter_member(parameter);
            Ok(RecordComponent {
                name: member,
                ty,
                required: parameter.required && !parameter.optional_filter,
                non_blank,
                length,
                positive,
                nonnegative,
            })
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    Ok(components)
}

/// The Java member an operation parameter is read through.
///
/// One owner: the `Input` record declares the component and three adapters
/// call the accessor, and a projection applied at one of those four is a
/// generated file that does not compile.
pub(crate) fn parameter_member(parameter: &OperationParameter) -> String {
    jails_model::lower_camel_case(&parameter.name)
}

/// What binds one component of a form-bound request.
///
/// **The declaration outranks the derivation.** `@BindParam` is derived from
/// the project's Jackson setting, which covers `userId` -> `user_id` and
/// cannot cover `id` -> `message_id`, because neither name follows from the
/// other -- so `--bind` states the pair the convention has no way to reach.
#[derive(Clone, Copy)]
pub(crate) struct Binder<'a> {
    pub(crate) model: &'a AppModel,
    pub(crate) declared: &'a [jails_model::ParameterBinding],
}

/// What this component is called on the wire, when that is not its own name.
///
/// **One answer for the record and its proof.** They are one fact, and a proof
/// posting the other name passes or fails for a reason that has nothing to do
/// with the endpoint.
pub(crate) fn wire_name(binder: Binder<'_>, member: &str) -> Option<String> {
    let Binder { model, declared } = binder;
    if let Some(wire) = declared
        .iter()
        .find(|binding| binding.parameter == member)
        .and_then(|binding| binding.wire_name.as_deref())
    {
        return (wire != member).then(|| wire.to_string());
    }
    if !snake_case_wire(model) {
        return None;
    }
    let mut wire = String::with_capacity(member.len() + 4);
    for character in member.chars() {
        if character.is_ascii_uppercase() && !wire.is_empty() {
            wire.push('_');
        }
        wire.push(character.to_ascii_lowercase());
    }
    (wire != member).then_some(wire)
}

/// The name a form field arrives under, where it is not the component's own.
///
/// **Spring's data binder has no naming strategy.** Jackson has one and
/// applies it to JSON without help, so a project whose wire is snake_case
/// still binds a *form* field called `userId` unless the component says
/// otherwise -- and a form post at a `@ModelAttribute` endpoint then delivers
/// `null` for every multi-word component, silently. `@BindParam` is what says
/// otherwise, and it is emitted only where the two spellings differ: an
/// annotation restating the default is noise in every one-word component.
///
/// Read off the model's own settings rather than a manifest:
/// `spring.jackson.property-naming-strategy` is where a project states this,
/// and jails does not need to be told again.
fn bind_param(
    binder: Binder<'_>,
    form: bool,
    member: &str,
    imports: &mut BTreeSet<String>,
) -> String {
    if !form {
        return String::new();
    }
    let Some(wire) = wire_name(binder, member) else {
        return String::new();
    };
    imports.insert("org.springframework.web.bind.annotation.BindParam".to_string());
    format!("@BindParam(\"{wire}\") ")
}

fn snake_case_wire(model: &AppModel) -> bool {
    model.settings.values().any(|setting| {
        setting.key == "spring.jackson.property-naming-strategy" && setting.value == "SNAKE_CASE"
    })
}

pub(crate) fn record_shape_from_components(
    type_name: &str,
    components: &[RecordComponent<'_>],
    imports: &mut BTreeSet<String>,
) -> String {
    record_shape_bound(type_name, components, imports, None)
}

pub(crate) fn record_shape_bound(
    type_name: &str,
    components: &[RecordComponent<'_>],
    imports: &mut BTreeSet<String>,
    binder: Option<Binder<'_>>,
) -> String {
    let declarations = components
        .iter()
        .map(|component| {
            let mut java = java_type_ref(component.ty, component.required, imports);
            if !component.required {
                imports.insert("java.util.Optional".to_string());
                java = format!("Optional<{java}>");
            }
            let bind = binder.map_or_else(String::new, |binder| {
                bind_param(binder, true, &component.name, imports)
            });
            format!("    {bind}{java} {}", component.name)
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let statements = components
        .iter()
        .flat_map(|component| record_validation::record_checks(component, imports))
        .collect::<Vec<_>>();
    let constructor = if statements.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n    public {type_name} {{\n{}\n    }}",
            statements
                .iter()
                .map(|statement| format!("        {statement}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    // **An empty component list is `()`, not a blank line between parens.** A
    // transition whose whole effect is a pinned constant carries nothing from
    // the caller, and `record Input(\n\n)` is a record with one thing wrong
    // with it that no reader would write.
    if components.is_empty() {
        return format!("public record {type_name}() {{{constructor}\n}}");
    }
    format!("public record {type_name}(\n{declarations}\n) {{{constructor}\n}}")
}
