//! Lower the enum ABI and its optional Spring conversion adapter.

use crate::CompileError;
use crate::emit_java::{JAVA_ROOT, render};
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, Entity, Package, StableId};
use std::collections::BTreeSet;

pub(crate) fn has_wire_values(entity: &Entity) -> bool {
    entity
        .enum_constants
        .iter()
        .any(|constant| constant.wire_name.is_some())
}

pub(crate) fn imports(entity: &Entity) -> BTreeSet<String> {
    if has_wire_values(entity) {
        BTreeSet::from([
            "com.fasterxml.jackson.annotation.JsonCreator".to_string(),
            "com.fasterxml.jackson.annotation.JsonValue".to_string(),
        ])
    } else {
        BTreeSet::new()
    }
}

pub(crate) fn shape(entity: &Entity) -> String {
    let type_name = &entity.names.java_type;
    if !has_wire_values(entity) {
        let values = entity
            .enum_constants
            .iter()
            .map(|constant| format!("    {}", constant.java_name))
            .collect::<Vec<_>>()
            .join(",\n");
        return format!("public enum {type_name} {{\n{values}\n}}");
    }
    let values = entity
        .enum_constants
        .iter()
        .map(|constant| format!("    {}(\"{}\")", constant.java_name, constant.wire_value()))
        .collect::<Vec<_>>()
        .join(",\n");
    let expected = entity
        .enum_constants
        .iter()
        .map(|constant| constant.wire_value())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "public enum {type_name} {{\n{values};\n\n    private final String wire;\n\n    {type_name}(String wire) {{\n        this.wire = wire;\n    }}\n\n    @JsonValue\n    public String wire() {{\n        return this.wire;\n    }}\n\n    @JsonCreator\n    public static {type_name} fromWire(String value) {{\n        for ({type_name} candidate : values()) {{\n            if (candidate.wire.equals(value)) {{\n                return candidate;\n            }}\n        }}\n        throw new IllegalArgumentException(\n                \"no {type_name} with wire value '\" + value + \"'; expected one of {expected}\");\n    }}\n}}"
    )
}

pub(crate) fn lower_converter(
    model: &AppModel,
    entity: &Entity,
) -> Result<(ProjectPath, RenderedFile), CompileError> {
    let package = model.project.package_for(Package::Web);
    let type_name = format!("{}Converter", entity.names.java_type);
    let enum_type = &entity.names.java_type;
    let imports = BTreeSet::from([
        format!("{}.{enum_type}", model.project.package_for(Package::Domain)),
        "org.springframework.core.convert.converter.Converter".to_string(),
        "org.springframework.stereotype.Component".to_string(),
    ]);
    let body = format!(
        "@Component\npublic final class {type_name} implements Converter<String, {enum_type}> {{\n\n    @Override\n    public {enum_type} convert(String source) {{\n        return {enum_type}.fromWire(source);\n    }}\n}}"
    );
    let artifact_id = format!("art_{}_enum-converter", entity.id.as_str());
    let rendered = render(&package, &imports, &body, &artifact_id);
    let package_path = package.replace('.', "/");
    let path = ProjectPath::parse(format!("{JAVA_ROOT}/{package_path}/{type_name}.java"))
        .map_err(CompileError::new)?;
    Ok((
        path,
        RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id,
                ejection_id: None,
                ejectable: false,
                semantic_ids: BTreeSet::from([entity.id.as_str().to_string()]),
                compiler_pass: "java-enum-converter".to_string(),
            },
        },
    ))
}
