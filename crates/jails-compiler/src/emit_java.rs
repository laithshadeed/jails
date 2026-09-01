//! Lower semantic facets into deterministic Java source units.

mod execution_context;
mod facet;
mod record_validation;
mod repository;
mod time_ordered_uuid;

use crate::CompileError;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Entity, EntityId, Facet, Field, FieldId, Operation, OperationKind,
    OperationParameter, Package, ParameterSource, StableId, TypeRef,
};
use std::collections::BTreeSet;

pub(crate) const JAVA_ROOT: &str = ".jails/generated/main/java";

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
    observed: &crate::emit::Observed<'_>,
) -> Result<(), CompileError> {
    let spring_boot = observed.spring_boot;
    crate::emit_unit::lower_and_emit(model, output, spring_boot)?;
    if let Some(unit) = execution_context::lower(model)? {
        output
            .insert(unit.path, unit.file)
            .map_err(CompileError::new)?;
    }
    if let Some(unit) = time_ordered_uuid::lower(model)? {
        output
            .insert(unit.path, unit.file)
            .map_err(CompileError::new)?;
    }
    for entity in model.entities.values().filter(|entity| entity.active) {
        for facet in &entity.facets {
            if *facet == Facet::Seed {
                for (path, file) in crate::emit_seed::lower(model, entity)? {
                    output.insert(path, file).map_err(CompileError::new)?;
                }
                continue;
            }
            // **The scaffold's HTTP surface, which is three files.** The
            // single-file arm below emits only the port -- an interface with
            // no implementation, no route and no caller -- so a canonical
            // scaffold serves nothing.
            if *facet == Facet::Http {
                for unit in crate::emit_resource_http::lower(model, entity, spring_boot)? {
                    output
                        .insert(unit.path, unit.file)
                        .map_err(CompileError::new)?;
                }
                continue;
            }
            if *facet == Facet::Dto {
                for unit in crate::emit_dto::lower(model, entity, spring_boot) {
                    let unit = unit?;
                    output
                        .insert(unit.path, unit.file)
                        .map_err(CompileError::new)?;
                }
                continue;
            }
            let unit = if *facet == Facet::Factory {
                crate::emit_factory::lower(model, entity)?
            } else {
                facet::lower_facet(model, entity, *facet, spring_boot)?
            };
            output
                .insert(unit.path, unit.file)
                .map_err(CompileError::new)?;
            // The companion test ships with the type, not as an opt-in: a
            // generated class nobody asserts anything about leaves the suite
            // green over it. See `emit_companion_test`.
            if let Some(unit) = crate::emit_companion_test::lower(model, entity, *facet)? {
                output
                    .insert(unit.path, unit.file)
                    .map_err(CompileError::new)?;
            }
            if spring_boot.is_some()
                && *facet == Facet::Enum
                && crate::emit_enum::has_wire_values(entity)
            {
                let (path, file) = crate::emit_enum::lower_converter(model, entity)?;
                output.insert(path, file).map_err(CompileError::new)?;
            }
        }
    }
    for operation in model.operations.values() {
        let unit = lower_operation(model, operation)?;
        output
            .insert(unit.path, unit.file)
            .map_err(CompileError::new)?;
    }
    // **A repository port always has an implementation**, and that is the
    // scaffold's whole promise: it produces a resource that *runs*. Gating the
    // in-memory adapter on the `fake` capability alone left a scaffolded
    // project with a `@Component` service constructor-injecting a port no bean
    // satisfies, so the application refused to start with "No qualifying bean
    // of type ...Repository" -- a project that compiles and cannot boot, which
    // is exactly the failure `jails beans` exists to report.
    //
    // The bean is still unique: `lower_db_repository` carries `@Repository`
    // when `db` is declared and this one drops the annotation, so the two
    // never both qualify for one injection point.
    let fake = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "fake");
    // **Declared, or already on the classpath.** A project that states
    // `spring-boot-starter-data-jdbc` in its own build has JDBC whether or not
    // the model declares `db`, and the bean has to be the adapter that talks
    // to it -- see `Observed::jdbc`. Emitting the in-memory one as the bean
    // there gave a project with a real database a `LinkedHashMap` beside a
    // query adapter reading from PostgreSQL: two answers to one question, and
    // the wrong one wired in.
    let declared = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "db");
    let stored = observed.jdbc || declared.is_some();
    if fake.is_some() || !stored {
        let owner = fake.map_or("cap_scaffold_default", |capability| capability.id.as_str());
        for entity in model
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Repository))
        {
            let unit = repository::lower_fake_repository(
                model,
                owner,
                entity,
                !stored && spring_boot.is_some(),
            )?;
            output
                .insert(unit.path, unit.file)
                .map_err(CompileError::new)?;
        }
    }
    // The owner is the `db` capability where there is one; where JDBC is only
    // an observation, the adapter belongs to the scaffold that asked for the
    // port, the same way the in-memory one does when nothing declares `fake`.
    if let Some(owner) = declared
        .map(|capability| capability.id.as_str())
        .or_else(|| observed.jdbc.then_some("cap_scaffold_default"))
    {
        for entity in model
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Repository))
        {
            let unit = repository::lower_db_repository(model, owner, entity)?;
            output
                .insert(unit.path, unit.file)
                .map_err(CompileError::new)?;
            // The tier that answers the question the adapter exists for. See
            // `lower_db_repository_it`.
            if let Some(unit) = repository::lower_db_repository_it(model, owner, entity)? {
                output
                    .insert(unit.path, unit.file)
                    .map_err(CompileError::new)?;
            }
        }
        // The search port's only implementation, and it belongs here rather
        // than beside the port: searching is a JDBC concern and the port
        // exists so a project that later moves search elsewhere replaces this
        // and nothing else.
        for entity in model
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Search))
        {
            let unit = repository::lower_search_adapter(model, owner, entity)?;
            output
                .insert(unit.path, unit.file)
                .map_err(CompileError::new)?;
        }
    }
    Ok(())
}

pub(crate) struct Unit {
    pub(crate) path: ProjectPath,
    pub(crate) file: RenderedFile,
}

fn lower_operation(model: &AppModel, operation: &Operation) -> Result<Unit, CompileError> {
    // **Only a form-bound route needs the annotation.** A JSON body reaches
    // Jackson, which applies the project's naming strategy itself; a form
    // reaches Spring's data binder, which has none.
    let binder = operation
        .route()
        .is_some_and(|route| route.consumes == Some(jails_model::RequestFormat::Form))
        .then(|| Binder {
            model,
            declared: operation.bindings(),
        });
    let (package, type_name, body, imports) = match &operation.kind {
        OperationKind::Command(command) => {
            let entity = entity(model, &command.on)?;
            let mut imports = BTreeSet::from([domain_import(model, entity)]);
            let components = input_components(model, operation, &mut imports)?;
            let input = indent(
                &record_shape_bound("Input", &components, &mut imports, binder),
                4,
            );
            let context = operation_context(model, entity, &mut imports);
            let type_name = with_suffix(&operation.names.java_type, "Command");
            let route = route_constant(command.route.as_deref());
            // **A resolved key can miss, and that is an outcome rather than a
            // fault.** The insert selects the foreign key out of the parent's
            // own row, so a caller naming a parent that is not there writes
            // nothing -- which is a 404, not a 500.
            let answer = if command.semantics.resolutions.is_empty() {
                entity.names.java_type.clone()
            } else {
                imports.insert("java.util.Optional".to_string());
                format!("Optional<{}>", entity.names.java_type)
            };
            let body = format!(
                "public interface {type_name} {{\n{route}\n    {answer} execute({context}Input input);\n\n{input}\n}}"
            );
            (Package::ApplicationCommands, type_name, body, imports)
        }
        OperationKind::Query(query) => {
            let entity = entity(model, &query.on)?;
            let mut imports =
                BTreeSet::from(["java.util.List".to_string(), domain_import(model, entity)]);
            let components = input_components(model, operation, &mut imports)?;
            let input = indent(
                &record_shape_bound("Input", &components, &mut imports, binder),
                4,
            );
            let context = operation_context(model, entity, &mut imports);
            let type_name = with_suffix(&operation.names.java_type, "Query");
            let route = route_constant(query.route.as_deref());
            let limit = query.semantics.limit.map_or_else(String::new, |limit| {
                format!("    int DEFAULT_LIMIT = {limit};\n\n")
            });
            let body = format!(
                "public interface {type_name} {{\n{route}{limit}    List<{}> execute({context}Input input);\n\n{input}\n}}",
                entity.names.java_type
            );
            (Package::ApplicationQueries, type_name, body, imports)
        }
        OperationKind::Transition(transition) => {
            let entity = entity(model, &transition.on)?;
            let key = transition_key(entity, transition)?;
            let mut imports = BTreeSet::from([domain_import(model, entity)]);
            let key_type = java_type(key, &mut imports);
            let key_member = &key.names.java_member;
            let components = input_components(model, operation, &mut imports)?;
            let input = indent(
                &record_shape_bound("Input", &components, &mut imports, binder),
                4,
            );
            let context = operation_context(model, entity, &mut imports);
            let type_name = with_suffix(&operation.names.java_type, "Transition");
            let route = route_constant(transition.route.as_deref());
            let expected = precondition(entity, transition)
                .map(|precondition| precondition.parameter(&mut imports))
                .unwrap_or_default();
            let body = format!(
                "public interface {type_name} {{\n{route}\n    {} execute({context}{key_type} {key_member}, Input input{expected});\n\n{input}\n}}",
                entity.names.java_type
            );
            (Package::ApplicationTransitions, type_name, body, imports)
        }
        OperationKind::Event(event) => {
            let mut imports = BTreeSet::new();
            let type_name = with_suffix(&operation.names.java_type, "Event");
            // **The linked parameters, not the flat `fields`.** The flat list
            // can only name fields of the target entity, so an event
            // declaring a component the row does not carry -- its own minted
            // `id`, the moment it happened -- rendered a record without it,
            // and the emitter that stages the payload then named an accessor
            // no record had. The linker folds `fields` into the parameters,
            // so this is the whole payload either way; the command's `Input`
            // has read it this way all along.
            let body = if event.semantics.parameters.is_empty() {
                let fields = event.on.as_ref().map_or_else(
                    || Ok(Vec::new()),
                    |entity_id| {
                        let entity = entity(model, entity_id)?;
                        fields(entity, &event.fields)
                    },
                )?;
                record_shape(&type_name, &fields, &mut imports)
            } else {
                record_shape_from_components(
                    &type_name,
                    &parameter_components(model, &event.semantics.parameters, &mut imports)?,
                    &mut imports,
                )
            };
            (Package::DomainEvents, type_name, body, imports)
        }
    };
    let package = model.project.package_for(package);
    let artifact_id = format!(
        "art_{}_{}",
        operation.id.as_str(),
        operation_kind_name(&operation.kind)
    );
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: false,
                semantic_ids: BTreeSet::from([operation.id.as_str().to_string()]),
                compiler_pass: "java-operations".to_string(),
            },
        },
    })
}

fn operation_context(model: &AppModel, entity: &Entity, imports: &mut BTreeSet<String>) -> String {
    if entity
        .fields
        .iter()
        .any(|field| field.semantics.scope.is_some())
    {
        imports.insert(format!(
            "{}.ExecutionContext",
            model.project.package_for(Package::Application)
        ));
        "ExecutionContext context, ".to_string()
    } else {
        String::new()
    }
}

fn facet_name(facet: Facet) -> &'static str {
    match facet {
        Facet::Enum => "enum",
        Facet::Record => "record",
        Facet::Factory => "factory",
        Facet::Dto => "dto",
        Facet::Repository => "repository",
        Facet::Service => "service",
        Facet::Http => "http",
        Facet::Events => "events",
        Facet::Search => "search",
        Facet::Seed => "seed",
    }
}

fn operation_kind_name(kind: &OperationKind) -> &'static str {
    match kind {
        OperationKind::Command(_) => "command",
        OperationKind::Query(_) => "query",
        OperationKind::Transition(_) => "transition",
        OperationKind::Event(_) => "event",
    }
}

pub(crate) fn entity<'a>(model: &'a AppModel, id: &EntityId) -> Result<&'a Entity, CompileError> {
    model
        .entities
        .get(id)
        .ok_or_else(|| CompileError::new(format!("linked operation references missing `{id}`")))
}

fn fields<'a>(entity: &'a Entity, ids: &[FieldId]) -> Result<Vec<&'a Field>, CompileError> {
    ids.iter()
        .map(|id| {
            entity.field(id).ok_or_else(|| {
                CompileError::new(format!(
                    "linked operation references missing field `{id}` on `{}`",
                    entity.id
                ))
            })
        })
        .collect()
}

pub(crate) fn domain_import(model: &AppModel, entity: &Entity) -> String {
    format!(
        "{}.{}",
        model.project.package_for(Package::Domain),
        entity.names.java_type
    )
}

/// What one operation's `Input` record declares, in order.
///
/// **One answer, because two readers need it and they must not differ.** The
/// record renderer builds the components from this, and `emit_http::proof`
/// builds the request that binds to them from the same list. They disagreed
/// before it existed: a query's `Input` takes entity fields, so its components
/// are `java_member` (`userId`), while a command with linked parameters takes
/// those, so its components are parameter labels (`user_id`). A test that
/// guessed either spelling sent `user_id=1` at a record declaring `userId` and
/// got a 400 -- `bugs.md` B48 in its second form.
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
        // field list cannot hold one -- so reading the flat list emitted an
        // `Input` missing that component while the adapter bound it, and the
        // endpoint answered over every row. Same choice the command arm above
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

fn record_shape(type_name: &str, fields: &[&Field], imports: &mut BTreeSet<String>) -> String {
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
/// `Input` in `application.queries` carrying a `MessageDirection` compiled
/// against a symbol it never imported. The entity's own record gets away with
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
    if model
        .entities
        .values()
        .any(|entity| &entity.names.java_type == name)
    {
        imports.insert(format!(
            "{}.{name}",
            model.project.package_for(Package::Domain)
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

fn parameter_components<'a>(
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
            // `Input` shipped `user_id` and `is_read` while a query's, built
            // from the field list, shipped `userId`. Two operation kinds
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
/// Read off the model's own settings rather than a manifest, for the reason
/// `sql_dialect` reads the driver: `spring.jackson.property-naming-strategy`
/// is where a project states this, and jails does not need to be told again.
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

fn record_shape_from_components(
    type_name: &str,
    components: &[RecordComponent<'_>],
    imports: &mut BTreeSet<String>,
) -> String {
    record_shape_bound(type_name, components, imports, None)
}

fn record_shape_bound(
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

fn route_constant(route: Option<&str>) -> String {
    route.map_or_else(String::new, |route| {
        format!("    String ROUTE = {};\n\n", java_string(route))
    })
}

fn java_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

pub(crate) fn with_suffix(value: &str, suffix: &str) -> String {
    if value.ends_with(suffix) {
        value.to_string()
    } else {
        format!("{value}{suffix}")
    }
}

/// Shift a rendered block right, leaving blank lines blank.
///
/// **An empty line gets no prefix**, or the indent turns it into trailing
/// whitespace -- which spotless removes, so a freshly generated project would
/// fail its own `jails check` on bytes jails wrote.
fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                format!("{prefix}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The component a transition addresses its row by.
///
/// **The primary key unless the author named another one.** `--select userId`
/// addresses the row by a different unique component, and four renderers have
/// to agree about which one it is: the port's `execute` parameter, the JDBC
/// adapter's `where`, the HTTP controller's path variable, and the proof that
/// expands it. Reading `select` separately in each of them is `bugs.md` B48's
/// shape, so it is read here.
///
/// A multi-column selector is refused where the update statement is rendered,
/// which is the renderer that cannot express one; this returns the first so
/// the port and the route still say something true about the rest.
/// The row version this transition's caller states, and how it states it.
///
/// **The version travels as `If-Match` and comes back as an `ETag`.** It used
/// to be a component of the request body, which is a bespoke spelling of a
/// thing HTTP already has -- and one no cache, proxy or client library
/// understands. So the port takes it as its own argument, the body does not
/// carry it, and the controller reads the header.
///
/// `None` when the declaration asked for no precondition, in which case the
/// linker has already refused a version parameter and there is nothing to
/// take.
pub(crate) fn precondition<'a>(
    entity: &'a Entity,
    transition: &jails_model::Transition,
) -> Option<Precondition<'a>> {
    let required = match transition.semantics.precondition? {
        jails_model::Precondition::Required => true,
        jails_model::Precondition::Optional => false,
        jails_model::Precondition::None => return None,
    };
    // The linker holds that an if-match transition's entity has exactly one
    // version field, so a miss here is a model that did not link rather than a
    // shape to render around.
    let field = entity.fields.iter().find(|field| field.semantics.version)?;
    Some(Precondition { field, required })
}

/// The version argument a precondition adds to the port.
pub(crate) struct Precondition<'a> {
    pub(crate) field: &'a Field,
    /// Whether the caller must send one. An optional precondition arrives as
    /// `null`, so its Java type is always the boxed one.
    pub(crate) required: bool,
}

impl Precondition<'_> {
    /// The Java type of the expected version, boxed when it may be absent.
    pub(crate) fn java_type(&self, imports: &mut BTreeSet<String>) -> String {
        let java = java_type(self.field, imports);
        if self.required {
            return java;
        }
        match java.as_str() {
            "long" => "Long".to_string(),
            "int" => "Integer".to_string(),
            "short" => "Short".to_string(),
            other => other.to_string(),
        }
    }

    /// The parameter this adds to `execute`, with its leading comma.
    pub(crate) fn parameter(&self, imports: &mut BTreeSet<String>) -> String {
        format!(", {} expectedVersion", self.java_type(imports))
    }
}

pub(crate) fn transition_key<'a>(
    entity: &'a Entity,
    transition: &jails_model::Transition,
) -> Result<&'a Field, CompileError> {
    match transition.semantics.select.first() {
        None => primary_key(entity),
        Some(selected) => entity
            .fields
            .iter()
            .find(|field| &field.id == selected)
            .ok_or_else(|| {
                CompileError::new(format!(
                    "linked entity `{}` does not declare selected field `{selected}`",
                    entity.id
                ))
            }),
    }
}

pub(crate) fn primary_key(entity: &Entity) -> Result<&Field, CompileError> {
    entity
        .fields
        .iter()
        .find(|field| field.primary_key)
        .ok_or_else(|| {
            CompileError::new(format!("linked entity `{}` has no primary key", entity.id))
        })
}

pub(crate) fn java_type(field: &Field, imports: &mut BTreeSet<String>) -> String {
    java_type_ref(&field.ty, field.required, imports)
}

pub(crate) fn java_type_ref(
    ty: &TypeRef,
    required: bool,
    imports: &mut BTreeSet<String>,
) -> String {
    match ty {
        TypeRef::Builtin(builtin) => builtin_java(*builtin, required, imports),
        TypeRef::External(qualified) => {
            if let Some((_, simple)) = qualified.rsplit_once('.') {
                imports.insert(qualified.clone());
                simple.to_string()
            } else {
                qualified.clone()
            }
        }
    }
}

pub(crate) fn primitive(ty: &TypeRef, required: bool) -> bool {
    required
        && matches!(ty, TypeRef::Builtin(builtin) if builtin.semantics().java_primitive.is_some())
}

pub(crate) struct RecordComponent<'a> {
    pub(crate) name: String,
    pub(crate) ty: &'a TypeRef,
    pub(crate) required: bool,
    non_blank: bool,
    length: Option<&'a jails_model::LengthRange>,
    positive: bool,
    nonnegative: bool,
}

fn builtin_java(builtin: BuiltinType, required: bool, imports: &mut BTreeSet<String>) -> String {
    let (name, import) = builtin.java_type(required);
    if let Some(import) = import {
        imports.insert(import.to_string());
    }
    name.to_string()
}

pub(crate) fn render(
    package: &str,
    imports: &BTreeSet<String>,
    body: &str,
    semantic_id: &str,
) -> String {
    let imports = if imports.is_empty() {
        String::new()
    } else {
        format!(
            "\n{}\n",
            imports
                .iter()
                .map(|import| format!("import {import};"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    format!(
        "// Generated by jails from {semantic_id}. Clean hand edits survive regeneration.\npackage {package};\n{imports}\n{body}\n"
    )
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
