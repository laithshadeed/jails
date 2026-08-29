//! Lower semantic facets into deterministic Java source units.

mod execution_context;
mod record_validation;
mod repository;
mod time_ordered_uuid;

use crate::CompileError;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Entity, EntityId, Facet, Field, FieldId, Operation, OperationKind,
    OperationParameter, ParameterSource, StableId, TypeRef,
};
use std::collections::BTreeSet;

pub(crate) const JAVA_ROOT: &str = ".jails/generated/main/java";

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
    spring_boot: bool,
) -> Result<(), CompileError> {
    crate::emit_unit::lower_and_emit(model, output)?;
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
            if *facet == Facet::Dto {
                for unit in crate::emit_dto::lower(model, entity) {
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
                lower_facet(model, entity, *facet)?
            };
            output
                .insert(unit.path, unit.file)
                .map_err(CompileError::new)?;
            if spring_boot && *facet == Facet::Enum && crate::emit_enum::has_wire_values(entity) {
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
    if let Some(capability) = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "fake")
    {
        for entity in model
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Repository))
        {
            let unit = repository::lower_fake_repository(model, capability.id.as_str(), entity)?;
            output
                .insert(unit.path, unit.file)
                .map_err(CompileError::new)?;
        }
    }
    if let Some(capability) = model
        .capabilities
        .values()
        .find(|capability| capability.kind == "db")
    {
        for entity in model
            .entities
            .values()
            .filter(|entity| entity.active && entity.facets.contains(&Facet::Repository))
        {
            let unit = repository::lower_db_repository(model, capability.id.as_str(), entity)?;
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

fn lower_facet(model: &AppModel, entity: &Entity, facet: Facet) -> Result<Unit, CompileError> {
    let domain_package = format!("{}.domain", model.project.base_package);
    let (package, type_name, body, mut imports) = match facet {
        Facet::Enum => (
            domain_package.clone(),
            entity.names.java_type.clone(),
            crate::emit_enum::shape(entity),
            crate::emit_enum::imports(entity),
        ),
        Facet::Record => {
            let mut imports = BTreeSet::new();
            let fields = entity.fields.values().collect::<Vec<_>>();
            (
                domain_package.clone(),
                entity.names.java_type.clone(),
                record_shape(&entity.names.java_type, &fields, &mut imports),
                imports,
            )
        }
        Facet::Factory => unreachable!("factory has a test-source backend"),
        Facet::Dto => unreachable!("dto has a multi-file backend"),
        Facet::Repository => {
            let package = format!("{}.repository", model.project.base_package);
            let primary_key = primary_key(entity)?;
            let mut imports = BTreeSet::from([
                "java.util.List".to_string(),
                "java.util.Optional".to_string(),
                format!("{domain_package}.{}", entity.names.java_type),
            ]);
            let key_type = java_type(primary_key, &mut imports);
            let type_name = format!("{}Repository", entity.names.java_type);
            let variable = lower_first(&entity.names.java_type);
            let body = format!(
                "public interface {type_name} {{\n\n    Optional<{}> findById({key_type} id);\n\n    List<{}> findAll();\n\n    {} save({} {variable});\n\n    boolean deleteById({key_type} id);\n\n    // Reader extensions belong below this stable boundary.\n}}",
                entity.names.java_type,
                entity.names.java_type,
                entity.names.java_type,
                entity.names.java_type,
            );
            (package, type_name, body, imports)
        }
        Facet::Service => {
            let package = format!("{}.service", model.project.base_package);
            let type_name = format!("{}Service", entity.names.java_type);
            let imports = BTreeSet::from([format!("{domain_package}.{}", entity.names.java_type)]);
            let body = format!(
                "public interface {type_name} {{\n\n    {} save({} value);\n}}",
                entity.names.java_type, entity.names.java_type
            );
            (package, type_name, body, imports)
        }
        Facet::Http => {
            let package = format!("{}.ports.http", model.project.base_package);
            let type_name = format!("{}HttpPort", entity.names.java_type);
            let imports = BTreeSet::from([format!("{domain_package}.{}", entity.names.java_type)]);
            let body = format!(
                "public interface {type_name} {{\n\n    {} create({} request);\n}}",
                entity.names.java_type, entity.names.java_type
            );
            (package, type_name, body, imports)
        }
        Facet::Events => {
            let package = format!("{}.ports.events", model.project.base_package);
            let type_name = format!("{}Events", entity.names.java_type);
            let imports = BTreeSet::from([format!("{domain_package}.{}", entity.names.java_type)]);
            let body = format!(
                "public interface {type_name} {{\n\n    void publish({} event);\n}}",
                entity.names.java_type
            );
            (package, type_name, body, imports)
        }
        Facet::Search => {
            let package = format!("{}.ports.search", model.project.base_package);
            let type_name = format!("{}Search", entity.names.java_type);
            let imports = BTreeSet::from([
                "java.util.List".to_string(),
                format!("{domain_package}.{}", entity.names.java_type),
            ]);
            let body = format!(
                "public interface {type_name} {{\n\n    List<{}> search(String query);\n}}",
                entity.names.java_type
            );
            (package, type_name, body, imports)
        }
    };
    imports.remove(&format!("{package}.{type_name}"));
    let artifact_id = format!("art_{}_{}", entity.id.as_str(), facet_name(facet));
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
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: "java-facets".to_string(),
            },
        },
    })
}

fn lower_operation(model: &AppModel, operation: &Operation) -> Result<Unit, CompileError> {
    let (package_suffix, type_name, body, imports) = match &operation.kind {
        OperationKind::Command(command) => {
            let entity = entity(model, &command.on)?;
            let mut imports = BTreeSet::from([domain_import(model, entity)]);
            let input = if command.semantics.parameters.is_empty() {
                let fields = fields(entity, &command.fields)?;
                record_shape("Input", &fields, &mut imports)
            } else {
                operation_record_shape(model, "Input", &command.semantics.parameters, &mut imports)?
            };
            let input = indent(&input, 4);
            let context = operation_context(model, entity, &mut imports);
            let type_name = with_suffix(&operation.names.java_type, "Command");
            let route = route_constant(command.route.as_deref());
            let body = format!(
                "public interface {type_name} {{\n{route}\n    {} execute({context}Input input);\n\n{input}\n}}",
                entity.names.java_type
            );
            ("application.commands", type_name, body, imports)
        }
        OperationKind::Query(query) => {
            let entity = entity(model, &query.on)?;
            let mut imports =
                BTreeSet::from(["java.util.List".to_string(), domain_import(model, entity)]);
            let fields = fields(entity, &query.filters)?;
            let input = indent(&record_shape("Input", &fields, &mut imports), 4);
            let context = operation_context(model, entity, &mut imports);
            let type_name = with_suffix(&operation.names.java_type, "Query");
            let route = route_constant(query.route.as_deref());
            let limit = query.limit.map_or_else(String::new, |limit| {
                format!("    int DEFAULT_LIMIT = {limit};\n\n")
            });
            let body = format!(
                "public interface {type_name} {{\n{route}{limit}    List<{}> execute({context}Input input);\n\n{input}\n}}",
                entity.names.java_type
            );
            ("application.queries", type_name, body, imports)
        }
        OperationKind::Transition(transition) => {
            let entity = entity(model, &transition.on)?;
            let primary_key = primary_key(entity)?;
            let mut imports = BTreeSet::from([domain_import(model, entity)]);
            let key_type = java_type(primary_key, &mut imports);
            let fields = fields(entity, &transition.fields)?;
            let input = indent(&record_shape("Input", &fields, &mut imports), 4);
            let context = operation_context(model, entity, &mut imports);
            let type_name = with_suffix(&operation.names.java_type, "Transition");
            let route = route_constant(transition.route.as_deref());
            let body = format!(
                "public interface {type_name} {{\n{route}\n    {} execute({context}{key_type} id, Input input);\n\n{input}\n}}",
                entity.names.java_type
            );
            ("application.transitions", type_name, body, imports)
        }
        OperationKind::Event(event) => {
            let mut imports = BTreeSet::new();
            let fields = event.on.as_ref().map_or_else(
                || Ok(Vec::new()),
                |entity_id| {
                    let entity = entity(model, entity_id)?;
                    fields(entity, &event.fields)
                },
            )?;
            let type_name = with_suffix(&operation.names.java_type, "Event");
            let body = record_shape(&type_name, &fields, &mut imports);
            ("domain.events", type_name, body, imports)
        }
    };
    let package = format!("{}.{}", model.project.base_package, package_suffix);
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
        .values()
        .any(|field| field.semantics.scope.is_some())
    {
        imports.insert(format!(
            "{}.application.ExecutionContext",
            model.project.base_package
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
            entity.fields.get(id).ok_or_else(|| {
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
        "{}.domain.{}",
        model.project.base_package, entity.names.java_type
    )
}

fn record_shape(type_name: &str, fields: &[&Field], imports: &mut BTreeSet<String>) -> String {
    let components = fields
        .iter()
        .map(|field| RecordComponent {
            name: &field.names.java_member,
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

fn operation_record_shape(
    model: &AppModel,
    type_name: &str,
    parameters: &[OperationParameter],
    imports: &mut BTreeSet<String>,
) -> Result<String, CompileError> {
    let components = parameters
        .iter()
        .map(|parameter| {
            let inherited = match &parameter.source {
                ParameterSource::Typed(_) => None,
                ParameterSource::Field(visible) => {
                    let owner = entity(model, &visible.entity)?;
                    let field = owner.fields.get(&visible.field).ok_or_else(|| {
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
            Ok(RecordComponent {
                name: &parameter.name,
                ty,
                required: parameter.required && !parameter.optional_filter,
                non_blank,
                length,
                positive,
                nonnegative,
            })
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    Ok(record_shape_from_components(
        type_name,
        &components,
        imports,
    ))
}

fn record_shape_from_components(
    type_name: &str,
    components: &[RecordComponent<'_>],
    imports: &mut BTreeSet<String>,
) -> String {
    let declarations = components
        .iter()
        .map(|component| {
            let mut java = java_type_ref(component.ty, component.required, imports);
            if !component.required {
                imports.insert("java.util.Optional".to_string());
                java = format!("Optional<{java}>");
            }
            format!("    {java} {}", component.name)
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

fn indent(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn primary_key(entity: &Entity) -> Result<&Field, CompileError> {
    entity
        .fields
        .values()
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

fn primitive(ty: &TypeRef, required: bool) -> bool {
    required
        && matches!(ty, TypeRef::Builtin(builtin) if builtin.semantics().java_primitive.is_some())
}

struct RecordComponent<'a> {
    name: &'a str,
    ty: &'a TypeRef,
    required: bool,
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
