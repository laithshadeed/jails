//! What a model may declare that this compiler cannot yet honour.
//!
//! Split out of `Compiler::compile` when that file crossed the largest-module
//! ceiling, and by a secret worth having its own file: every check here runs
//! *before* anything is lowered, and every one exists because the alternative
//! is silence. A capability with no backend, a component kind with no emitter,
//! a delivery policy the emitters do not implement — each of these compiles
//! perfectly well into *less* than it says, and less-than-it-says is the one
//! outcome a compiler must not have.
//!
//! So the rule for adding one is the same as the rule for adding an emitter:
//! if the model can state it and the compiler cannot render it, it belongs
//! here, with a `fix:` line naming what the reader can do instead.

use crate::{CompileError, component_kind_is_emitted, emit_capability};
use jails_contracts::WorkspaceSnapshot;
use jails_model::AppModel;

pub(crate) fn preflight(
    snapshot: &WorkspaceSnapshot,
    next_model: &AppModel,
) -> Result<(), CompileError> {
    if snapshot.project.java_release != next_model.project.java_release {
        return Err(CompileError::new(format!(
            "captured Java release {} disagrees with model release {}; recapture or update the model",
            snapshot.project.java_release, next_model.project.java_release
        )));
    }
    if snapshot.project.base_package != next_model.project.base_package {
        return Err(CompileError::new(format!(
            "captured base package `{}` disagrees with model package `{}`; recapture or update the model",
            snapshot.project.base_package, next_model.project.base_package
        )));
    }
    if (next_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "api")
        || next_model
            .units
            .values()
            .any(|unit| unit.kind == jails_model::UnitKind::Controller))
        && snapshot.project.spring_boot.is_none()
    {
        return Err(CompileError::new(
            "canonical `api` adapters require a captured Spring Boot project\n       fix: add Spring Boot to the build or remove the `api` capability",
        ));
    }
    if next_model.capabilities.values().any(|capability| {
        matches!(
            capability.kind.as_str(),
            "h2" | "actuator"
                | "cache"
                | "cors"
                | "observability"
                | "security"
                | "sse"
                | "redis"
                | "mail"
        )
    }) && snapshot.project.spring_boot.is_none()
    {
        return Err(CompileError::new(
            "canonical Spring capability packs require a captured Spring Boot project\n       fix: add the capability to a Spring project or remove it from the model",
        ));
    }
    if let Some((kind, minimum, actual)) = next_model.capabilities.values().find_map(|capability| {
        let minimum = emit_capability::minimum_boot(&capability.kind)?;
        let actual = emit_capability::boot_major(snapshot.project.spring_boot.as_deref())?;
        (actual < minimum).then_some((capability.kind.as_str(), minimum, actual))
    }) {
        return Err(CompileError::new(format!(
            "canonical `{kind}` requires Spring Boot {minimum}+ but the captured project uses Boot {actual}\n       fix: raise the Spring Boot version or remove the `{kind}` capability"
        )));
    }
    if next_model.units.values().any(|unit| {
        matches!(
            unit.kind,
            // `Strategy` is deliberately not here. A service and a
            // controller are an annotation with a class around them and mean
            // nothing without Spring; a strategy is a port, its
            // implementations and an evaluator that takes them as a
            // constructor argument, all of which compile on plain Maven --
            // which is what the legacy generator has always emitted there.
            jails_model::UnitKind::Service | jails_model::UnitKind::Controller
        )
    }) && snapshot.project.spring_boot.is_none()
    {
        return Err(CompileError::new(
            "canonical Spring source units require a captured Spring Boot project\n       fix: add Spring Boot to the build or use a plain class/interface unit",
        ));
    }
    if next_model
        .entities
        .values()
        .any(|entity| entity.active && entity.facets.contains(&jails_model::Facet::Dto))
        && snapshot.project.spring_boot.is_none()
    {
        return Err(CompileError::new(
            "canonical DTO facets require a captured Spring Boot project\n       fix: add Spring Boot to the build or remove the `dto` facet",
        ));
    }
    if let Some(component) = next_model
        .components
        .values()
        .find(|component| !component_kind_is_emitted(component.kind))
    {
        return Err(CompileError::new(format!(
            "canonical `component {}` has no compiler backend yet\n       fix: remove the declaration, or generate `{}` on a legacy project until its emitter lands",
            component.kind.label(),
            component.kind.label()
        )));
    }
    // The database backend renders `JdbcClient` adapters annotated
    // `@Repository`, so it is Spring-only in the same way `api` and the
    // Spring capability packs are. Without this it emitted that Java into
    // a project that cannot compile it, *and* spliced four versionless
    // dependencies into a pom with no parent to manage them -- which
    // Maven refuses to read at all, `validate` included.
    if next_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
        && snapshot.project.spring_boot.is_none()
    {
        return Err(CompileError::new(
            "canonical `storage postgres` renders Spring JDBC adapters and requires a captured Spring Boot project\n       fix: add Spring Boot to the build, or choose `storage none` and write the persistence by hand",
        ));
    }
    if next_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "fast-test")
        && snapshot.project.spring_boot.is_none()
    {
        return Err(CompileError::new(
            "canonical fast-test ownership currently requires captured Spring Boot dependency management\n       fix: import a Spring Boot or JUnit BOM project before using `test --fast`",
        ));
    }
    Ok(())
}
