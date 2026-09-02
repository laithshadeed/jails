//! `component handler <Name>`: HTTP with no framework in it.
//!
//! The JDK's own `HttpHandler`, so a project with no Spring on the classpath
//! still has a way to serve a resource. Thin by construction: it binds,
//! routes, and maps outcomes to status codes, and holds no rules — which is
//! what lets the same service be driven from a CLI.
//!
//! **`ApiError` belongs to every handler in the model**, so it is emitted once
//! from [`super::lower_and_emit`] rather than by each one — the same rule
//! `SchedulingConfig` follows, and for the same reason: a managed tree refuses
//! two units writing one path, so a per-handler emitter would compile and then
//! fail on the second declaration.

use super::{Emitted, Package, package};
use crate::CompileError;
use crate::emit_java::JavaUnit;
use jails_contracts::{FileKind, FileMode, ProjectPath, Provenance, RenderedFile};
use jails_model::{AppModel, ComponentKind, StableId};
use std::collections::BTreeSet;

const API_ERROR: crate::Template = crate::template!("spring/api_error_java.java");
const API_ERROR_TEST: crate::Template = crate::template!("spring/api_error_test_java.java");

/// The one error envelope every handler in this model renders failures
/// through, and its test.
pub(super) fn envelope(
    model: &AppModel,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    let owners = model
        .components
        .values()
        .filter(|component| component.kind == ComponentKind::Handler)
        .map(|component| component.id.as_str().to_string())
        .collect::<BTreeSet<_>>();
    if owners.is_empty() {
        return Ok(Vec::new());
    }
    let domain = package(model, Package::Domain);
    [
        ("art_app_api_error", "ApiError", API_ERROR, false),
        (
            "art_app_api_error_test",
            "ApiErrorTest",
            API_ERROR_TEST,
            true,
        ),
    ]
    .into_iter()
    .map(|(artifact, type_name, template, test)| {
        let root = if test {
            super::TEST_ROOT
        } else {
            super::MAIN_ROOT
        };
        let path = ProjectPath::parse(format!(
            "{root}/{}/{type_name}.java",
            domain.replace('.', "/")
        ))
        .map_err(CompileError::new)?;
        Ok(Emitted {
            path,
            file: RenderedFile {
                bytes: JavaUnit::from_source(
                    &template.resolve(templates)?.replace("{{domain}}", &domain),
                )
                .render(artifact)
                .into_bytes(),
                kind: if test {
                    FileKind::JavaTest
                } else {
                    FileKind::JavaMain
                },
                mode: FileMode::Regular,
                provenance: Provenance {
                    artifact_id: artifact.to_string(),
                    ejection_id: None,
                    ejectable: true,
                    semantic_ids: owners.clone(),
                    compiler_pass: "components".to_string(),
                },
            },
        })
    })
    .collect()
}
