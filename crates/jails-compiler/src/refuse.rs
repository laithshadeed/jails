//! What a model may declare that this compiler cannot yet honour.
//!
//! Every check here runs *before* anything is lowered, and every one exists
//! because the alternative is silence. A capability with no backend, a
//! component kind with no emitter,
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
            "`api` is a Spring Boot capability and this project has no Spring Boot parent\n       fix: add Spring Boot to the build, or `jails add http` for a framework-free HTTP client",
        ));
    }
    let spring_only = next_model.capabilities.values().find(|capability| {
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
    });
    if let Some(capability) = spring_only
        && snapshot.project.spring_boot.is_none()
    {
        // Named, because a command listing several capabilities refuses over
        // one of them and a reader cannot retry what the message will not say.
        return Err(CompileError::new(format!(
            "`{}` is a Spring Boot capability and this project has no Spring Boot parent\n       fix: add Spring Boot to the build, or remove `{}` from the model",
            capability.kind, capability.kind
        )));
    }
    if let Some((kind, minimum, needs, actual)) =
        next_model.capabilities.values().find_map(|capability| {
            let (minimum, needs) = emit_capability::minimum_boot(&capability.kind)?;
            let actual = emit_capability::boot_major(snapshot.project.spring_boot.as_deref())?;
            (actual < minimum).then_some((capability.kind.as_str(), minimum, needs, actual))
        })
    {
        return Err(CompileError::new(format!(
            "canonical `{kind}` writes `{needs}`, which needs Spring Boot {minimum}, and this is a Spring Boot {actual} project\n       fix: raise the Spring Boot version, or keep to what compiles there -- `jails g scaffold`, `jails g usecase`, `jails g enum` and `jails add cors` all do"
        )));
    }
    // **An operation somebody injects needs something to implement it.** The
    // compiler emits a port for every linked `command`, `query` and
    // `transition` and a `JdbcClient` adapter behind it -- and only the `db`
    // capability brings that adapter. Without one the port is a declaration
    // nothing constructs, which is harmless until the `api` capability writes
    // a controller that takes it: then the application compiles and fails to
    // start on "no qualifying bean of type". A scaffold's in-memory repository
    // is not the missing bean -- it stores rows, and an operation is a
    // statement.
    let capability_kinds = |kind: &str| {
        next_model
            .capabilities
            .values()
            .any(|capability| capability.kind == kind)
    };
    if capability_kinds("api")
        && !capability_kinds("db")
        && let Some(operation) = next_model
            .operations
            .values()
            .find(|operation| !matches!(operation.kind, jails_model::OperationKind::Event(_)))
    {
        return Err(CompileError::new(format!(
            "canonical operation `{}` answers a route through a `JdbcClient` adapter, and this model declares no SQL storage -- so nothing implements the port its controller takes\n       fix: run `jails add db`, or remove the `api` capability",
            operation.label
        )));
    }
    if next_model.units.values().any(|unit| {
        matches!(
            unit.kind,
            // `Strategy` is deliberately not here. A service and a
            // controller are an annotation with a class around them and mean
            // nothing without Spring; a strategy is a port, its
            // implementations and an evaluator that takes them as a
            // constructor argument, all of which compile on plain Maven.
            jails_model::UnitKind::Service | jails_model::UnitKind::Controller
        )
    }) && snapshot.project.spring_boot.is_none()
    {
        return Err(CompileError::new(
            "canonical Spring source units require a captured Spring Boot project\n       fix: add Spring Boot to the build or use a plain class/interface unit",
        ));
    }
    // **A capitalised field type is a type this project owns, and this is
    // where that claim is checked.** Unchecked, `g scaffold Book author:Author`
    // emits a record naming `Author` and leaves the project unable to compile
    // a file the reader never wrote -- the exact failure jails exists to
    // remove, and one no later command could explain, because nothing in the
    // model is wrong. The reader's own declarations are observed once during
    // capture; the model's entities and enums are its own.
    for entity in next_model.entities.values().filter(|entity| entity.active) {
        for field in &entity.fields {
            let jails_model::TypeRef::External(name) = &field.ty else {
                continue;
            };
            // **Everything the next model will put in the tree, plus what the
            // reader already has.** A sealed interface or a strategy port is a
            // unit rather than an entity, and a component can carry a type of
            // its own -- all three are emitted by this same plan, so a field
            // naming one is naming something that will be there. What the
            // check is for is the name nothing anywhere accounts for.
            if name.contains('.')
                || snapshot.external_types.types.contains_key(name)
                || next_model
                    .entities
                    .values()
                    .any(|declared| &declared.names.java_type == name)
                || next_model
                    .units
                    .values()
                    .any(|unit| &unit.java_type == name)
                || next_model
                    .components
                    .values()
                    .any(|component| &component.name == name)
            {
                continue;
            }
            return Err(CompileError::new(format!(
                "`{}:{name}` names a type nothing declares: `{name}` is neither in this model nor in your own sources\n       fix: declare it with `jails g record {name} ...` or `jails g enum {name} ...`, write it yourself, or use one of jails' lowercase types",
                field.label
            )));
        }
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
    // **The database itself is not Spring's; the adapters for it are.**
    // `java.sql` is in the JDK, so `storage postgres` on a plain Maven project
    // is the driver, Flyway and a compose service -- all pinned, because with
    // no parent to manage a version Maven refuses to read the pom at all,
    // `validate` included. What needs Spring is the *repository adapter*,
    // which is a `JdbcClient` class annotated `@Repository`, so the refusal is
    // about the entity that asks for one rather than about the storage axis.
    if next_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "db")
    {
        match snapshot.project.spring_boot.as_deref() {
            None => {
                if let Some(entity) = next_model.entities.values().find(|entity| {
                    entity.active && entity.facets.contains(&jails_model::Facet::Repository)
                }) {
                    return Err(CompileError::new(format!(
                        "`{}` asks for a repository adapter, which the compiler renders as a Spring `JdbcClient` bean, and this project has no Spring Boot parent\n       fix: add Spring Boot to the build, or drop the `repository` facet and write the persistence against `java.sql` by hand",
                        entity.label
                    )));
                }
            }
            Some(version) => {
                // **The floor is 3.1, and below it the refusal has to name the
                // module.** `spring-boot-testcontainers` and
                // `spring-boot-docker-compose` exist from there; on an older
                // project the coordinates this capability declares resolve to
                // nothing and the build stops resolving at all -- worse than
                // the state before the command ran. A refusal that merely said
                // "requires a captured Spring Boot project" would be telling a
                // Boot 2.7 reader to add the Spring Boot they already have.
                if let Some((major, minor)) = boot_major_minor(version)
                    && (major, minor) < (3, 1)
                {
                    return Err(CompileError::new(format!(
                        "`db` wires Testcontainers through `spring-boot-testcontainers`, and this project \
                 is Spring Boot {major}.{minor}.\n       \
                 That module and `spring-boot-docker-compose` arrived in Spring Boot 3.1; on this \
                 project the spliced coordinates would resolve to nothing and the build would \
                 stop resolving altogether -- a worse state than before the command ran.\n       \
                 fix: `jails add sqlite`, `jails add h2`, `jails g migration` and `jails g repo` \
                 work on this project. Raising the Boot version is the other way."
                    )));
                }
            }
        }
    }
    if next_model
        .capabilities
        .values()
        .any(|capability| capability.kind == "fast-test")
        && snapshot.project.spring_boot.is_none()
        && snapshot.project.junit.is_none()
    {
        return Err(CompileError::new(
            "`test --fast` runs JUnit's console launcher, and this project declares no JUnit version for it to match\n       fix: declare a JUnit dependency with an explicit version, or a Spring Boot parent that manages one",
        ));
    }
    Ok(())
}

/// `"2.7.18"` as `(2, 7)`. `None` when the string is not a version, which the
/// caller reads as "cannot tell" and lets through -- the same posture the
/// capture takes when no build file names a Boot version at all.
fn boot_major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}
