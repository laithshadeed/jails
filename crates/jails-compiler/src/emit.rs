//! The order the emitters run in, and the facts they read off the snapshot.
//!
//! `Compiler::compile` decides *what* the desired state is; this decides who
//! renders it. Every external fact an emitter needs is a field of
//! [`WorkspaceSnapshot`] -- `snapshot.project` for the build's facts,
//! `snapshot.template_overrides` for the reader's templates -- read directly,
//! so a renderer that needs one more fact asks capture for it rather than
//! widening a second copy kept here.

use crate::{CompileError, emit_capability, emit_component, emit_http, emit_java, emit_operation};
use jails_contracts::{FileKind, ProjectFacts, ProjectPath, RenderedTree, WorkspaceSnapshot};

/// Whether `JdbcClient` can be resolved in this project.
///
/// Spring's `spring-jdbc` is what declares it, and three starters bring it in:
/// the JDBC starter, the data-JDBC starter, and the JPA starter above them.
/// Named rather than matched on `jdbc` appearing anywhere in a coordinate, so
/// a project's own `com.example:jdbc-utils` is not read as Spring's.
///
/// **A model can say `storage none` over a project that has a database.**
/// The repository port always gets exactly one bean, and which adapter it is
/// depends on whether `JdbcClient` can be resolved at all. Reading it off the
/// declared `db` capability alone gives a Gradle project carrying
/// `spring-boot-starter-data-jdbc` an in-memory bean beside a query adapter
/// talking to its real database.
pub(crate) fn jdbc_on_classpath(project: &ProjectFacts) -> bool {
    const PROVIDERS: [&str; 4] = [
        "org.springframework:spring-jdbc",
        "org.springframework.boot:spring-boot-starter-jdbc",
        "org.springframework.boot:spring-boot-starter-data-jdbc",
        "org.springframework.boot:spring-boot-starter-data-jpa",
    ];
    PROVIDERS
        .iter()
        .any(|provider| project.dependencies.contains(*provider))
}

/// Whether JSpecify is on this project's classpath.
///
/// **A package annotated `@NullMarked` that cannot resolve the annotation is
/// a compile error in a file the reader did not ask for**, so the
/// `package-info.java` beside every generated package is conditional on the
/// artifact actually being a dependency. It is worth writing when it is:
/// without a package-level opt-in JSpecify reads the package as "unspecified
/// nullness" and a nullness checker has nothing to check -- package level is
/// the only level JSpecify offers.
pub(crate) fn jspecify_on_classpath(project: &ProjectFacts) -> bool {
    project.dependencies.contains("org.jspecify:jspecify")
}

/// One pass over the model, writing its part of the desired tree.
type Pass =
    fn(&jails_model::AppModel, &mut RenderedTree, &WorkspaceSnapshot) -> Result<(), CompileError>;

/// The passes that walk [`crate::recipe::Recipe`] rows: each looks a node's
/// recipe up and renders its files through the one loop.
const RECIPE_WALKS: &[Pass] = &[
    // The 22 capability packs, and the project files of ci, docker, k8s,
    // loadtest and format beside them.
    emit_capability::emit,
    // Twelve component kinds as rows; http-sink and durable-job as functions.
    emit_component::emit,
    // event: the publisher, the handler port, the listener and their proofs.
    crate::emit_messaging::emit,
    // A command's outbox: store, sink, worker.
    emit_operation::outbox::emit,
];

/// The passes that are still functions: emitters that build Java from the
/// model's structure -- a record's components, a query's SQL, a proof's
/// request -- and have not been reduced to rows and fragments.
///
/// **This is the number to watch.** Every one that becomes a recipe walk
/// comes off this list, and `docs/60-abstraction.md` S60.3 keeps the count;
/// the test below holds the two together.
const FUNCTIONS: &[Pass] = &[
    // The one-file entity facets are `Recipe<Entity>` rows inside this pass
    // (`emit_java::entity`); what keeps it a function is the rest: the
    // multi-file facets (dto, http, seed), the units (class, interface,
    // service, sealed, strategy, controller, test), the operation ports and
    // the repository adapters.
    emit_java::emit,
    // command, query and transition.
    emit_operation::emit,
    // association.
    crate::emit_relation::emit,
    // The HTTP proofs of every routed operation.
    emit_http::emit,
    // The architecture test.
    crate::emit_architecture::emit,
];

/// The whole desired tree, as the passes that write it.
///
/// **The two tables are the answer to "what renders kind X".** Their order is
/// free: every pass writes its own paths and a tree refuses two units at one
/// path, so the only sequencing is that `package_infos` runs over the
/// finished tree and `tidy_java` last.
pub(crate) fn emit(
    model: &jails_model::AppModel,
    output: &mut RenderedTree,
    snapshot: &WorkspaceSnapshot,
) -> Result<(), CompileError> {
    for pass in RECIPE_WALKS.iter().chain(FUNCTIONS) {
        pass(model, output, snapshot)?;
    }
    package_infos(output, jspecify_on_classpath(&snapshot.project))?;
    tidy_java(output);
    Ok(())
}

/// One `package-info.java` for every package this compile writes main Java
/// into.
///
/// **Emitted here rather than per generator**: it is a fact about a
/// *package*, and a rule twenty renderers have to remember is a rule that
/// decays the first time somebody adds one. Running over the finished tree
/// also makes "one per package" structural rather than something to check.
///
/// Main sources only. A test package is not part of anyone's API and a
/// nullness checker configured over `src/test` is a choice the reader makes.
fn package_infos(output: &mut RenderedTree, jspecify: bool) -> Result<(), CompileError> {
    if !jspecify {
        return Ok(());
    }
    let root = format!("{}/", jails_contracts::SourceRoot::MainJava.path());
    let packages: std::collections::BTreeSet<String> = output
        .files
        .iter()
        .filter(|(_, file)| file.kind == FileKind::JavaMain)
        .filter_map(|(path, _)| {
            let rest = path.as_str().strip_prefix(root.as_str())?;
            let (directory, _) = rest.rsplit_once('/')?;
            Some(directory.to_string())
        })
        .collect();
    for directory in packages {
        let path = ProjectPath::parse(format!("{root}{directory}/package-info.java"))
            .map_err(CompileError::new)?;
        if output.files.contains_key(&path) {
            continue;
        }
        let package = directory.replace('/', ".");
        let bytes = format!(
            "/**\n\
             \x20* Every reference type in this package is non-null unless it is explicitly\n\
             \x20* annotated {{@code @Nullable}}.\n\
             \x20*\n\
             \x20* <p>This is a package-level opt-in because that is the only level JSpecify\n\
             \x20* offers: without it the package is \"unspecified nullness\" and a nullness\n\
             \x20* checker has nothing to check.\n\
             \x20*/\n\
             @NullMarked\n\
             package {package};\n\n\
             import org.jspecify.annotations.NullMarked;\n"
        )
        .into_bytes();
        output
            .insert(
                path,
                jails_contracts::RenderedFile {
                    kind: FileKind::JavaMain,
                    mode: jails_contracts::FileMode::Regular,
                    bytes,
                    provenance: jails_contracts::Provenance {
                        artifact_id: format!("art_package_info_{}", package.replace('.', "_")),
                        ejection_id: None,
                        ejectable: false,
                        semantic_ids: std::collections::BTreeSet::new(),
                        compiler_pass: "package-nullness".to_string(),
                    },
                },
            )
            .map_err(CompileError::new)?;
    }
    Ok(())
}

/// Collapse runs of blank lines in every emitted Java file.
///
/// **One rule in one place.** palantir-java-format removes a doubled blank
/// line, so leaving one in means `add format` -- which jails installs itself
/// -- fails `jails check` on a project whose every line jails wrote.
/// Templates produce them without meaning to: a placeholder line that
/// substitutes to nothing leaves an empty line that is invisible in the
/// template and obvious in the output.
///
/// A blank line inside a text block is data and is left alone, which is why
/// this counts `"""` fences rather than trimming unconditionally.
fn tidy_java(output: &mut RenderedTree) {
    for file in output.files.values_mut() {
        if !matches!(file.kind, FileKind::JavaMain | FileKind::JavaTest) {
            continue;
        }
        let Ok(text) = std::str::from_utf8(&file.bytes) else {
            continue;
        };
        let trailing_newline = text.ends_with('\n');
        let mut kept: Vec<&str> = Vec::new();
        let mut in_text_block = false;
        let mut previous_blank = false;
        for line in text.lines() {
            let blank = line.trim().is_empty();
            if blank && previous_blank {
                continue;
            }
            kept.push(line);
            if line.matches("\"\"\"").count() % 2 == 1 {
                in_text_block = !in_text_block;
            }
            previous_blank = blank && !in_text_block;
        }
        let mut tidied = kept.join("\n");
        if trailing_newline {
            tidied.push('\n');
        }
        if tidied.as_bytes() != file.bytes.as_slice() {
            file.bytes = tidied.into_bytes();
        }
    }
}

pub(crate) fn compose_path(snapshot: &WorkspaceSnapshot) -> Result<ProjectPath, CompileError> {
    if let Some(path) = snapshot
        .accepted_projection
        .as_ref()
        .and_then(|projection| {
            projection.reader_facets.values().find(|facet| {
                matches!(
                    facet.kind,
                    jails_contracts::ReaderFacetKind::ComposeService { .. }
                )
            })
        })
        .map(|facet| facet.path.clone())
    {
        return Ok(path);
    }
    for candidate in [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yml",
        "docker-compose.yaml",
    ] {
        let path = ProjectPath::parse(candidate).map_err(CompileError::new)?;
        if snapshot.files.contains_key(&path) {
            return Ok(path);
        }
    }
    ProjectPath::parse("compose.yaml").map_err(CompileError::new)
}

#[cfg(test)]
mod tests {
    /// The number `docs/60-abstraction.md` S60.3 states for the passes that
    /// are still functions. A pass that becomes a recipe walk lowers this
    /// beside the doc; one that grows back raises it and says why.
    #[test]
    fn five_passes_are_still_functions() {
        assert_eq!(super::FUNCTIONS.len(), 5);
        assert_eq!(super::RECIPE_WALKS.len(), 4);
    }
}
