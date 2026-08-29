//! Lower one entity DTO facet into its request, response, and contract test.
//!
//! The three files are one semantic projection but three merge histories.
//! Their artifact ids therefore differ even though field evolution recompiles
//! them in one plan. DTOs are managed ABI: reader edits merge, but ownership
//! cannot be transferred away from the compiler.

use crate::{CompileError, emit_java};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Entity, Field, StableId};
use std::collections::BTreeSet;

const JAVA_TEST_ROOT: &str = ".jails/generated/test/java";

pub(crate) fn lower(
    model: &AppModel,
    entity: &Entity,
) -> Vec<Result<emit_java::Unit, CompileError>> {
    vec![
        request(model, entity),
        response(model, entity),
        contract_test(model, entity),
    ]
}

fn request(model: &AppModel, entity: &Entity) -> Result<emit_java::Unit, CompileError> {
    let package = format!("{}.web", model.project.base_package);
    let type_name = format!("{}Request", entity.names.java_type);
    let artifact_id = format!("art_{}_dto_request", entity.id.as_str());
    let mut imports = BTreeSet::from([emit_java::domain_import(model, entity)]);
    let components = components(entity, &mut imports, true);
    if entity.fields.values().any(|field| !field.required) {
        imports.insert("java.util.Optional".to_string());
    }
    let arguments = entity
        .fields
        .values()
        .map(|field| {
            let name = &field.names.java_member;
            if field.required {
                format!("                {name}")
            } else {
                format!("                Optional.ofNullable({name})")
            }
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let record = &entity.names.java_type;
    let body = format!(
        "public record {type_name}(\n{components}\n) {{\n\n    public {record} toDomain() {{\n        return new {record}(\n{arguments});\n    }}\n}}"
    );
    unit(
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaMain,
        "dto-request",
        entity,
    )
}

fn response(model: &AppModel, entity: &Entity) -> Result<emit_java::Unit, CompileError> {
    let package = format!("{}.web", model.project.base_package);
    let type_name = format!("{}Response", entity.names.java_type);
    let artifact_id = format!("art_{}_dto_response", entity.id.as_str());
    let mut imports = BTreeSet::from([emit_java::domain_import(model, entity)]);
    let components = components(entity, &mut imports, false);
    let variable = lower_first(&entity.names.java_type);
    let arguments = entity
        .fields
        .values()
        .map(|field| {
            let accessor = format!("{variable}.{}()", field.names.java_member);
            if field.required {
                format!("                {accessor}")
            } else {
                format!("                {accessor}.orElse(null)")
            }
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let record = &entity.names.java_type;
    let body = format!(
        "public record {type_name}(\n{components}\n) {{\n\n    public static {type_name} fromDomain({record} {variable}) {{\n        return new {type_name}(\n{arguments});\n    }}\n}}"
    );
    unit(
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaMain,
        "dto-response",
        entity,
    )
}

fn contract_test(model: &AppModel, entity: &Entity) -> Result<emit_java::Unit, CompileError> {
    let package = format!("{}.web", model.project.base_package);
    let record = &entity.names.java_type;
    let type_name = format!("{record}DtoTest");
    let artifact_id = format!("art_{}_dto_test", entity.id.as_str());
    let imports = BTreeSet::from([
        "java.util.Arrays".to_string(),
        "java.util.List".to_string(),
        "org.junit.jupiter.api.Test".to_string(),
        "static org.junit.jupiter.api.Assertions.assertEquals".to_string(),
    ]);
    let names = entity
        .fields
        .values()
        .map(|field| format!("\"{}\"", field.names.java_member))
        .collect::<Vec<_>>()
        .join(", ");
    let body = format!(
        "final class {type_name} {{\n\n    @Test\n    void requestAndResponseExposeCanonicalComponents() {{\n        var expected = List.of({names});\n        assertEquals(expected, componentNames({record}Request.class));\n        assertEquals(expected, componentNames({record}Response.class));\n    }}\n\n    private static List<String> componentNames(Class<?> type) {{\n        return Arrays.stream(type.getRecordComponents())\n                .map(component -> component.getName())\n                .toList();\n    }}\n}}"
    );
    unit(
        package,
        type_name,
        artifact_id,
        imports,
        body,
        FileKind::JavaTest,
        "dto-contract-test",
        entity,
    )
}

fn components(entity: &Entity, imports: &mut BTreeSet<String>, validation: bool) -> String {
    entity
        .fields
        .values()
        .map(|field| {
            let annotation = if validation {
                validation_annotation(field, imports)
            } else {
                None
            };
            let java = emit_java::java_type(field, imports);
            let name = &field.names.java_member;
            match annotation {
                Some(annotation) => format!("    {annotation} {java} {name}"),
                None => format!("    {java} {name}"),
            }
        })
        .collect::<Vec<_>>()
        .join(",\n")
}

fn validation_annotation(field: &Field, imports: &mut BTreeSet<String>) -> Option<&'static str> {
    if !field.required || primitive(field) {
        return None;
    }
    if field.non_blank {
        imports.insert("jakarta.validation.constraints.NotBlank".to_string());
        Some("@NotBlank")
    } else {
        imports.insert("jakarta.validation.constraints.NotNull".to_string());
        Some("@NotNull")
    }
}

fn primitive(field: &Field) -> bool {
    field.required
        && matches!(
            field.ty,
            jails_model::TypeRef::Builtin(builtin) if builtin.semantics().java_primitive.is_some()
        )
}

#[allow(clippy::too_many_arguments)]
fn unit(
    package: String,
    type_name: String,
    artifact_id: String,
    imports: BTreeSet<String>,
    body: String,
    kind: FileKind,
    compiler_pass: &str,
    entity: &Entity,
) -> Result<emit_java::Unit, CompileError> {
    let rendered = emit_java::render(&package, &imports, &body, &artifact_id);
    let root = match kind {
        FileKind::JavaMain => emit_java::JAVA_ROOT,
        FileKind::JavaTest => JAVA_TEST_ROOT,
        _ => unreachable!("DTOs lower only Java source"),
    };
    let path = ProjectPath::parse(format!(
        "{root}/{}/{}.java",
        package.replace('.', "/"),
        type_name
    ))
    .map_err(CompileError::new)?;
    Ok(emit_java::Unit {
        path,
        file: RenderedFile {
            kind,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: false,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: compiler_pass.to_string(),
            },
        },
    })
}

fn lower_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_lowercase().to_string() + characters.as_str()
    })
}
