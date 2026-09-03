//! Managed, framework-neutral ABI for compiler-supplied operation context.

use super::{JAVA_ROOT, JavaUnit, Unit};
use crate::Diagnostic;
use jails_contracts::{FileKind, FileMode, Provenance, RenderedFile};
use jails_model::{AppModel, Package, StableId};
use std::collections::BTreeSet;

pub(super) fn lower(model: &AppModel) -> Result<Option<Unit>, Diagnostic> {
    if !model.entities.values().any(|entity| {
        entity
            .fields
            .iter()
            .any(|field| field.semantics.scope.is_some())
    }) {
        return Ok(None);
    }

    let package = model.project.package_for(Package::Application);
    let type_name = "ExecutionContext";
    let artifact_id = "art_app_execution_context";
    let imports = BTreeSet::from(["java.util.Map".to_string(), "java.util.Objects".to_string()]);
    let body = r#"public record ExecutionContext(Map<String, String> claims) {

    public ExecutionContext {
        claims = Map.copyOf(Objects.requireNonNull(claims, "claims"));
    }

    public String claim(String name) {
        var value = claims.get(name);
        if (value == null || value.isBlank()) {
            throw new IllegalArgumentException("missing execution-context claim `" + name + "`");
        }
        return value;
    }
}"#;
    let rendered = JavaUnit::new(&package, &imports, body).render(artifact_id);
    let path = crate::refuse::project_path(format!(
        "{JAVA_ROOT}/{}/{}.java",
        package.replace('.', "/"),
        type_name
    ))?;
    Ok(Some(Unit {
        path,
        file: RenderedFile {
            kind: FileKind::JavaMain,
            mode: FileMode::Regular,
            bytes: rendered.into_bytes(),
            provenance: Provenance {
                artifact_id: artifact_id.to_string(),
                ejection_id: None,
                ejectable: false,
                semantic_ids: BTreeSet::from([model.project.id.as_str().to_string()]),
                compiler_pass: "java-execution-context".to_string(),
            },
        },
    }))
}
