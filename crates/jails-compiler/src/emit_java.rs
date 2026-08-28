//! Lower semantic facets into deterministic Java source units.

mod record_validation;

use crate::CompileError;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile, RenderedTree};
use jails_model::{
    AppModel, BuiltinType, Entity, EntityId, Facet, Field, FieldId, Operation, OperationKind,
    StableId, TypeRef,
};
use std::collections::BTreeSet;

pub(crate) const JAVA_ROOT: &str = ".jails/generated/main/java";

pub(crate) fn lower_and_emit(
    model: &AppModel,
    output: &mut RenderedTree,
    spring_boot: bool,
) -> Result<(), CompileError> {
    crate::emit_unit::lower_and_emit(model, output)?;
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
            let unit = lower_fake_repository(model, capability.id.as_str(), entity)?;
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
            let unit = lower_db_repository(model, capability.id.as_str(), entity)?;
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
            let fields = fields(entity, &command.fields)?;
            let input = indent(&record_shape("Input", &fields, &mut imports), 4);
            let type_name = with_suffix(&operation.names.java_type, "Command");
            let route = route_constant(command.route.as_deref());
            let body = format!(
                "public interface {type_name} {{\n{route}\n    {} execute(Input input);\n\n{input}\n}}",
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
            let type_name = with_suffix(&operation.names.java_type, "Query");
            let route = route_constant(query.route.as_deref());
            let limit = query.limit.map_or_else(String::new, |limit| {
                format!("    int DEFAULT_LIMIT = {limit};\n\n")
            });
            let body = format!(
                "public interface {type_name} {{\n{route}{limit}    List<{}> execute(Input input);\n\n{input}\n}}",
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
            let type_name = with_suffix(&operation.names.java_type, "Transition");
            let route = route_constant(transition.route.as_deref());
            let body = format!(
                "public interface {type_name} {{\n{route}\n    {} execute({key_type} id, Input input);\n\n{input}\n}}",
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

fn lower_fake_repository(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Unit, CompileError> {
    let primary_key = primary_key(entity)?;
    let package = format!("{}.adapters.memory", model.project.base_package);
    let type_name = format!("InMemory{}Repository", entity.names.java_type);
    let repository = format!(
        "{}.repository.{}Repository",
        model.project.base_package, entity.names.java_type
    );
    let mut imports = BTreeSet::from([
        repository,
        domain_import(model, entity),
        "java.util.LinkedHashMap".to_string(),
        "java.util.List".to_string(),
        "java.util.Map".to_string(),
        "java.util.Optional".to_string(),
    ]);
    let key_type = java_type(primary_key, &mut imports);
    let record = &entity.names.java_type;
    let key = &primary_key.names.java_member;
    let body = format!(
        "public final class {type_name} implements {record}Repository {{\n\n    private final Map<{key_type}, {record}> rows = new LinkedHashMap<>();\n\n    @Override\n    public Optional<{record}> findById({key_type} id) {{\n        return Optional.ofNullable(rows.get(id));\n    }}\n\n    @Override\n    public List<{record}> findAll() {{\n        return List.copyOf(rows.values());\n    }}\n\n    @Override\n    public {record} save({record} value) {{\n        rows.put(value.{key}(), value);\n        return value;\n    }}\n\n    @Override\n    public boolean deleteById({key_type} id) {{\n        return rows.remove(id) != null;\n    }}\n}}"
    );
    let artifact_id = format!("art_{capability_id}_{}_repository", entity.id.as_str());
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
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    entity.id.as_str().to_string(),
                ]),
                compiler_pass: "capability-fake".to_string(),
            },
        },
    })
}

fn lower_db_repository(
    model: &AppModel,
    capability_id: &str,
    entity: &Entity,
) -> Result<Unit, CompileError> {
    let primary_key = primary_key(entity)?;
    let package = format!("{}.adapters.jdbc", model.project.base_package);
    let type_name = format!("Jdbc{}Repository", entity.names.java_type);
    let repository = format!(
        "{}.repository.{}Repository",
        model.project.base_package, entity.names.java_type
    );
    let mut imports = BTreeSet::from([
        repository,
        domain_import(model, entity),
        "java.util.List".to_string(),
        "java.util.Optional".to_string(),
        "org.springframework.jdbc.core.simple.JdbcClient".to_string(),
        "org.springframework.stereotype.Repository".to_string(),
    ]);
    let record = &entity.names.java_type;
    let key_type = java_type(primary_key, &mut imports);
    let table = &entity.names.sql_table;
    let key_column = &primary_key.names.sql_column;
    let columns = entity
        .fields
        .values()
        .map(|field| field.names.sql_column.as_str())
        .collect::<Vec<_>>();
    let column_list = columns.join(", ");
    let values = columns
        .iter()
        .map(|column| format!(":{column}"))
        .collect::<Vec<_>>()
        .join(", ");
    let updates = entity
        .fields
        .values()
        .filter(|field| field.id != primary_key.id)
        .map(|field| {
            format!(
                "{} = excluded.{}",
                field.names.sql_column, field.names.sql_column
            )
        })
        .collect::<Vec<_>>();
    let updates = if updates.is_empty() {
        format!("{key_column} = excluded.{key_column}")
    } else {
        updates.join(", ")
    };
    let params = entity
        .fields
        .values()
        .map(|field| {
            let member = &field.names.java_member;
            let value = if field.required {
                format!("value.{member}()")
            } else {
                format!("value.{member}().orElse(null)")
            };
            format!(
                "\n                .param(\"{}\", {value})",
                field.names.sql_column
            )
        })
        .collect::<String>();
    let body = format!(
        "@Repository\npublic final class {type_name} implements {record}Repository {{\n\n    private final JdbcClient jdbc;\n\n    public {type_name}(JdbcClient jdbc) {{\n        this.jdbc = jdbc;\n    }}\n\n    @Override\n    public Optional<{record}> findById({key_type} id) {{\n        return jdbc.sql(\"select {column_list} from {table} where {key_column} = :id\")\n                .param(\"id\", id)\n                .query({record}.class)\n                .optional();\n    }}\n\n    @Override\n    public List<{record}> findAll() {{\n        return jdbc.sql(\"select {column_list} from {table} order by {key_column}\")\n                .query({record}.class)\n                .list();\n    }}\n\n    @Override\n    public {record} save({record} value) {{\n        return jdbc.sql(\"insert into {table} ({column_list}) values ({values}) on conflict ({key_column}) do update set {updates} returning {column_list}\"){params}\n                .query({record}.class)\n                .single();\n    }}\n\n    @Override\n    public boolean deleteById({key_type} id) {{\n        return jdbc.sql(\"delete from {table} where {key_column} = :id\")\n                .param(\"id\", id)\n                .update() > 0;\n    }}\n}}",
        key_type = key_type,
    );
    let artifact_id = format!("art_{capability_id}_{}_repository", entity.id.as_str());
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
                ejectable: true,
                semantic_ids: BTreeSet::from([
                    capability_id.to_string(),
                    entity.id.as_str().to_string(),
                ]),
                compiler_pass: "capability-db".to_string(),
            },
        },
    })
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
        .map(|field| {
            let mut java = java_type(field, imports);
            if !field.required {
                imports.insert("java.util.Optional".to_string());
                java = format!("Optional<{java}>");
            }
            format!("    {java} {}", field.names.java_member)
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let statements = fields
        .iter()
        .flat_map(|field| record_validation::record_checks(field, imports))
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
    format!("public record {type_name}(\n{components}\n) {{{constructor}\n}}")
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
    match &field.ty {
        TypeRef::Builtin(builtin) => builtin_java(*builtin, field.required, imports),
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

fn primitive(field: &Field) -> bool {
    field.required
        && matches!(
            field.ty,
            TypeRef::Builtin(
                BuiltinType::Integer
                    | BuiltinType::Long
                    | BuiltinType::Double
                    | BuiltinType::Boolean
            )
        )
}

fn builtin_java(builtin: BuiltinType, required: bool, imports: &mut BTreeSet<String>) -> String {
    let (name, import) = match builtin {
        BuiltinType::String => ("String", None),
        BuiltinType::Integer if required => ("int", None),
        BuiltinType::Integer => ("Integer", None),
        BuiltinType::Long if required => ("long", None),
        BuiltinType::Long => ("Long", None),
        BuiltinType::Double if required => ("double", None),
        BuiltinType::Double => ("Double", None),
        BuiltinType::Decimal => ("BigDecimal", Some("java.math.BigDecimal")),
        BuiltinType::Boolean if required => ("boolean", None),
        BuiltinType::Boolean => ("Boolean", None),
        BuiltinType::Uuid => ("UUID", Some("java.util.UUID")),
        BuiltinType::Date => ("LocalDate", Some("java.time.LocalDate")),
        BuiltinType::DateTime => ("LocalDateTime", Some("java.time.LocalDateTime")),
        BuiltinType::Instant => ("Instant", Some("java.time.Instant")),
        BuiltinType::Duration => ("Duration", Some("java.time.Duration")),
        BuiltinType::Uri => ("URI", Some("java.net.URI")),
        BuiltinType::Path => ("Path", Some("java.nio.file.Path")),
        BuiltinType::ZoneId => ("ZoneId", Some("java.time.ZoneId")),
        BuiltinType::Currency => ("Currency", Some("java.util.Currency")),
        BuiltinType::Bytes => ("byte[]", None),
    };
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
