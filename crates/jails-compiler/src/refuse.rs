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
//!
//! # The compiler's shared refusals
//!
//! Below [`preflight`] are the constructors for the refusal *families* -- the
//! ones the crate's two hundred and sixteen refusal sites would otherwise
//! each spell for themselves. A family saying one thing gets one `compile-*`
//! code behind one constructor, which is what
//! `every_diagnostic_code_is_unique_and_kebab_case` in `tests/architecture/`
//! holds: thirty-five path refusals under `compile-path-invalid`,
//! twenty-eight duplicate emissions under `compile-duplicate-emission`, and
//! the thirty-four the linker's own invariants produce.
//!
//! **The path a compiler diagnostic carries is the model path of the
//! declaration it is about** -- `$.entities.Note.fields.title`, the same
//! coordinate the linker writes. Where the emitter holds the declaration's
//! stable id and not its owner it carries that instead, because an id
//! resolves to exactly one declaration and a section path (`$.operations`)
//! resolves to none. Where the subject is not a declaration at all -- a path
//! this render built, the tree it emitted into -- it is that path or the
//! tree.

use crate::{Diagnostic, component_kind_is_emitted, emit_capability};
use jails_contracts::{ProjectPath, WorkspaceSnapshot};
use jails_model::AppModel;

pub(crate) fn preflight(
    snapshot: &WorkspaceSnapshot,
    next_model: &AppModel,
) -> Result<(), Diagnostic> {
    if snapshot.project.java_release != next_model.project.java_release {
        return Err(Diagnostic::without_a_fix(
            "compile-captured-release-mismatch",
            "$.project.java_release",
            format!(
                "captured Java release {} disagrees with model release {}; recapture or update the model",
                snapshot.project.java_release, next_model.project.java_release
            ),
        ));
    }
    if snapshot.project.base_package != next_model.project.base_package {
        return Err(Diagnostic::without_a_fix(
            "compile-captured-package-mismatch",
            "$.project.base_package",
            format!(
                "captured base package `{}` disagrees with model package `{}`; recapture or update the model",
                snapshot.project.base_package, next_model.project.base_package
            ),
        ));
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
        return Err(Diagnostic::new(
            "compile-api-needs-spring-boot",
            "$.capabilities.api",
            "`api` is a Spring Boot capability and this project has no Spring Boot parent",
            "add Spring Boot to the build, or `jails add http` for a framework-free HTTP client",
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
        return Err(Diagnostic::new(
            "compile-capability-needs-spring-boot",
            format!("$.capabilities.{}", capability.kind),
            format!(
                "`{}` is a Spring Boot capability and this project has no Spring Boot parent",
                capability.kind
            ),
            format!(
                "add Spring Boot to the build, or remove `{}` from the model",
                capability.kind
            ),
        ));
    }
    if let Some((kind, minimum, needs, actual)) =
        next_model.capabilities.values().find_map(|capability| {
            let (minimum, needs) = emit_capability::minimum_boot(&capability.kind)?;
            let actual = emit_capability::boot_major(snapshot.project.spring_boot.as_deref())?;
            (actual < minimum).then_some((capability.kind.as_str(), minimum, needs, actual))
        })
    {
        return Err(Diagnostic::new(
            "compile-capability-needs-newer-boot",
            format!("$.capabilities.{kind}"),
            format!(
                "canonical `{kind}` writes `{needs}`, which needs Spring Boot {minimum}, and this is a Spring Boot {actual} project"
            ),
            "raise the Spring Boot version, or keep to what compiles there -- `jails g scaffold`, `jails g usecase`, `jails g enum` and `jails add cors` all do",
        ));
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
        return Err(Diagnostic::new(
            "compile-operation-without-storage-adapter",
            format!("$.operations.{}", operation.label),
            format!(
                "canonical operation `{}` answers a route through a `JdbcClient` adapter, and this model declares no SQL storage -- so nothing implements the port its controller takes",
                operation.label
            ),
            "run `jails add db`, or remove the `api` capability",
        ));
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
        return Err(Diagnostic::new(
            "compile-spring-unit-needs-spring-boot",
            "$.units",
            "canonical Spring source units require a captured Spring Boot project",
            "add Spring Boot to the build or use a plain class/interface unit",
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
            return Err(Diagnostic::new(
                "compile-field-type-undeclared",
                format!("$.entities.{}.fields.{}", entity.label, field.label),
                format!(
                    "`{}:{name}` names a type nothing declares: `{name}` is neither in this model nor in your own sources",
                    field.label
                ),
                format!(
                    "declare it with `jails g record {name} ...` or `jails g enum {name} ...`, write it yourself, or use one of jails' lowercase types"
                ),
            ));
        }
    }
    if next_model
        .entities
        .values()
        .any(|entity| entity.active && entity.facets.contains(&jails_model::Facet::Dto))
        && snapshot.project.spring_boot.is_none()
    {
        return Err(Diagnostic::new(
            "compile-dto-facet-needs-spring-boot",
            "$.entities",
            "canonical DTO facets require a captured Spring Boot project",
            "add Spring Boot to the build or remove the `dto` facet",
        ));
    }
    if let Some(component) = next_model
        .components
        .values()
        .find(|component| !component_kind_is_emitted(component.kind))
    {
        return Err(Diagnostic::new(
            "compile-component-without-backend",
            format!("$.components.{}", component.label),
            format!(
                "canonical `component {}` has no compiler backend yet",
                component.kind.label()
            ),
            format!(
                "remove the declaration, or generate `{}` on a legacy project until its emitter lands",
                component.kind.label()
            ),
        ));
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
                    return Err(Diagnostic::new(
                        "compile-repository-facet-needs-spring-boot",
                        format!("$.entities.{}", entity.label),
                        format!(
                            "`{}` asks for a repository adapter, which the compiler renders as a Spring `JdbcClient` bean, and this project has no Spring Boot parent",
                            entity.label
                        ),
                        "add Spring Boot to the build, or drop the `repository` facet and write the persistence against `java.sql` by hand",
                    ));
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
                    return Err(Diagnostic::new(
                        "compile-db-needs-boot-3-1",
                        "$.capabilities.db",
                        format!(
                            "`db` wires Testcontainers through `spring-boot-testcontainers`, and this project \
                 is Spring Boot {major}.{minor}.\n       \
                 That module and `spring-boot-docker-compose` arrived in Spring Boot 3.1; on this \
                 project the spliced coordinates would resolve to nothing and the build would \
                 stop resolving altogether -- a worse state than before the command ran."
                        ),
                        "`jails add sqlite`, `jails add h2`, `jails g migration` and `jails g repo` \
                 work on this project. Raising the Boot version is the other way.",
                    ));
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
        return Err(Diagnostic::new(
            "compile-fast-test-needs-junit",
            "$.capabilities.fast-test",
            "`test --fast` runs JUnit's console launcher, and this project declares no JUnit version for it to match",
            "declare a JUnit dependency with an explicit version, or a Spring Boot parent that manages one",
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

/// A path this compiler built that is not a canonical project path.
///
/// No `fix:`: the value came from the model and a layout rule rather than
/// from anything the reader typed, so the next step is whatever
/// [`ProjectPath::parse`] found wrong with it and naming one would be advice
/// jails cannot stand behind.
pub(crate) fn invalid_path(value: impl Into<String>, message: String) -> Diagnostic {
    Diagnostic::without_a_fix("compile-path-invalid", value, message)
}

/// A canonical project path, or the diagnostic that this is not one.
///
/// The one lift of [`ProjectPath::parse`] in this crate, so the thirty-four
/// emitters that build a path out of a source root, a package and a type name
/// share one code rather than spelling thirty-four.
pub(crate) fn project_path(value: impl Into<String>) -> Result<ProjectPath, Diagnostic> {
    let value = value.into();
    ProjectPath::parse(value.clone()).map_err(|message| invalid_path(value, message))
}

/// Two emitters that rendered into the same place.
///
/// One code for the family -- a file at a path another unit already emitted,
/// a reader facet under an id another already published -- because both are
/// the same fault: the render produced one artifact twice. The path or the id
/// is in the sentence and the subject is the tree they collided in. No
/// `fix:`, because a collision between two of jails' own emitters is not
/// something the reader can resolve.
pub(crate) fn duplicate_emission(message: String) -> Diagnostic {
    Diagnostic::without_a_fix("compile-duplicate-emission", "$.generated", message)
}

/// An id in the linked model that resolves to nothing.
///
/// The compiler reads a model the linker has already accepted, so each of
/// these is an invariant of *that* pass rather than something the reader
/// stated: a query whose entity id is not in `entities`, a parameter whose
/// field id is not on the record. They are one refusal -- this model does not
/// agree with itself -- with the broken edge in the sentence, and they carry
/// no `fix:` for the reason [`Diagnostic::without_a_fix`] exists: there is no
/// step to name that the linker was not already supposed to have taken.
pub(crate) fn unlinked(path: impl Into<String>, message: impl Into<String>) -> Diagnostic {
    Diagnostic::without_a_fix("compile-unlinked-reference", path, message)
}

/// The same fault as [`unlinked`], where the sentence already names a repair.
///
/// A separate code because it is a separate sentence: these seven were
/// written with a `fix:` line, and adopting this contract rewrites no
/// message -- folding them into [`unlinked`] would either drop that line or
/// invent it on the twenty-seven that never had one.
pub(crate) fn broken_link(path: impl Into<String>, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        "compile-model-link-broken",
        path,
        message,
        "repair the linked model before compiling",
    )
}

/// Removing `db` while the accepted schema still holds tables.
///
/// **The fix names the command, not the policy.** "Retire every table through
/// an explicit schema policy" is true and leaves the reader to find out which
/// command spells a policy; the entities holding the tables are known here, so
/// the line is the command they type, per entity, with the two policies it
/// takes. Three at most, because a fix line is one sentence and a project with
/// forty tables does not need forty commands to see the shape of the answer.
pub(crate) fn storage_abandoned<'a>(holders: impl Iterator<Item = &'a str>) -> Diagnostic {
    let holders: Vec<&str> = holders.collect();
    let named = &holders[..holders.len().min(3)];
    let commands = match named.is_empty() {
        true => "`jails destroy scaffold <Entity> --storage drop`".to_string(),
        false => named
            .iter()
            .map(|entity| format!("`jails destroy scaffold {entity} --storage drop`"))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let rest = match holders.len() - named.len() {
        0 => String::new(),
        more => format!(" and {more} more"),
    };
    Diagnostic::new(
        "compile-storage-abandoned",
        "$.capabilities.db",
        "removing canonical `db` would abandon accepted storage",
        format!(
            "retire each accepted table first -- {commands}{rest}, or `--storage preserve` to keep the rows -- then remove `db`"
        ),
    )
}
