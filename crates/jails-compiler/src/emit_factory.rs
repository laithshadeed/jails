//! Entity-derived mutable test-data builders.

use crate::emit_companion_test::JAVA_TEST_ROOT;
use crate::{CompileError, emit_java};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, BuiltinType, Entity, Field, Package, StableId, TypeRef};
use std::collections::BTreeSet;

pub(crate) fn lower(
    model: &AppModel,
    entity: &Entity,
) -> Result<super::emit_java::Unit, CompileError> {
    let package = crate::emit_java::entity_package(model, entity, Package::Testkit);
    let type_name = format!("{}Factory", entity.names.java_type);
    let artifact_id = format!("art_{}_factory", entity.id.as_str());
    let mut imports = BTreeSet::from([emit_java::domain_import(model, entity)]);
    let body = body(entity, &type_name, &mut imports);
    let rendered = emit_java::render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_TEST_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok(super::emit_java::Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaTest,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: true,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: "java-test-factory".to_string(),
            },
        },
    })
}

fn body(entity: &Entity, type_name: &str, imports: &mut BTreeSet<String>) -> String {
    let record = &entity.names.java_type;
    let fields = entity.fields.iter().collect::<Vec<_>>();
    let declarations = fields
        .iter()
        .map(|field| {
            let ty = declared_type(field, imports);
            let sample = sample(field, imports).unwrap_or_else(|| "null".to_string());
            format!("    private {ty} {} = {sample};", field.names.java_member)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let methods = fields
        .iter()
        .map(|field| {
            let ty = declared_type(field, imports);
            let name = &field.names.java_member;
            format!(
                "    public {type_name} with{}({ty} value) {{\n        this.{name} = value;\n        return this;\n    }}",
                upper_first(name)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let guards = fields
        .iter()
        .filter(|field| field.required && sample(field, imports).is_none())
        .map(|field| {
            let name = &field.names.java_member;
            format!(
                "        if ({name} == null) {{\n            throw new IllegalStateException(\"{type_name} needs {name}\");\n        }}"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let arguments = fields
        .iter()
        .map(|field| format!("                {}", field.names.java_member))
        .collect::<Vec<_>>()
        .join(",\n");
    let guards = if guards.is_empty() {
        String::new()
    } else {
        format!("{guards}\n")
    };
    format!(
        "/** Mutable test-data builder for {{@link {record}}}. */\npublic final class {type_name} {{\n\n    // State derived from canonical entity components.\n{declarations}\n\n    public static {type_name} a{record}() {{\n        return new {type_name}();\n    }}\n\n    // Fluent overrides derived from canonical entity components.\n{methods}\n\n    public {record} build() {{\n{guards}        return new {record}(\n{arguments});\n    }}\n}}"
    )
}

fn declared_type(field: &Field, imports: &mut BTreeSet<String>) -> String {
    let java = emit_java::java_type(field, imports);
    if field.required {
        java
    } else {
        imports.insert("java.util.Optional".to_string());
        format!("Optional<{java}>")
    }
}

fn sample(field: &Field, imports: &mut BTreeSet<String>) -> Option<String> {
    if !field.required {
        imports.insert("java.util.Optional".to_string());
        return Some("Optional.empty()".to_string());
    }
    match &field.ty {
        TypeRef::Builtin(builtin) => Some(builtin_sample(*builtin)),
        TypeRef::External(_) => None,
    }
}

fn builtin_sample(builtin: BuiltinType) -> String {
    builtin.semantics().sample.to_string()
}

fn upper_first(value: &str) -> String {
    let mut characters = value.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + characters.as_str()
    })
}
