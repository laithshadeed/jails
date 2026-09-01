//! What the emitters need from the workspace, and the order they run in.
//!
//! Split out of `lib.rs` by secret the second time that file crossed the
//! largest-module ceiling. `Compiler::compile` decides *what* the desired
//! state is; this decides who renders it and what each renderer is told about
//! a workspace it may not look at itself.
//!
//! Keeping them apart is what makes the next observed fact cheap: it is a
//! field on [`Observed`] and a line in [`Observed::of`], rather than another
//! parameter threaded through four signatures.

use crate::{CompileError, emit_capability, emit_component, emit_http, emit_java, emit_operation};
use jails_contracts::{FileKind, ProjectPath, RenderedTree, WorkspaceSnapshot};

/// The workspace facts emission needs and a pure compiler may not observe.
///
/// A value rather than three more parameters, for the reason `spring::Slice`
/// is one on the legacy side: every one of these is captured once and consumed
/// together, and threading them individually is how a signature reaches eight
/// arguments one honest addition at a time.
pub(crate) struct Observed<'a> {
    /// The Boot version the project declares, if it is a Spring project.
    pub spring_boot: Option<&'a str>,
    /// Where this project keeps its compose file.
    pub compose_path: &'a ProjectPath,
    /// Whether the project ships `mvnw`, so generated CI and container builds
    /// invoke the build the way the project actually offers it.
    pub maven_wrapper: bool,
    /// Whether JSpecify is on this project's classpath.
    ///
    /// **A package annotated `@NullMarked` that cannot resolve the annotation
    /// is a compile error in a file the reader did not ask for**, so the
    /// `package-info.java` beside every generated package is conditional on
    /// the artifact actually being a dependency. It is worth writing when it
    /// is: without a package-level opt-in JSpecify reads the package as
    /// "unspecified nullness" and a nullness checker has nothing to check --
    /// package level is the only level JSpecify offers.
    pub jspecify: bool,
    /// Whether Spring's JDBC is on this project's classpath.
    ///
    /// **A model can say `storage none` over a project that has a database.**
    /// The repository port always gets exactly one bean, and which adapter it
    /// is depends on whether `JdbcClient` -- from `spring-jdbc`, which the JDBC
    /// and data-JDBC starters both bring -- can be resolved at all. Reading it
    /// off the declared `db` capability alone gave a Gradle project carrying
    /// `spring-boot-starter-data-jdbc` an in-memory bean beside a query adapter
    /// talking to its real database.
    pub jdbc: bool,
}

/// Whether `JdbcClient` can be resolved in this project.
///
/// Spring's `spring-jdbc` is what declares it, and three starters bring it in:
/// the JDBC starter, the data-JDBC starter, and the JPA starter above them.
/// Named rather than matched on `jdbc` appearing anywhere in a coordinate, so
/// a project's own `com.example:jdbc-utils` is not read as Spring's.
pub(crate) fn jdbc_on_classpath(snapshot: &WorkspaceSnapshot) -> bool {
    const PROVIDERS: [&str; 4] = [
        "org.springframework:spring-jdbc",
        "org.springframework.boot:spring-boot-starter-jdbc",
        "org.springframework.boot:spring-boot-starter-data-jdbc",
        "org.springframework.boot:spring-boot-starter-data-jpa",
    ];
    PROVIDERS
        .iter()
        .any(|provider| snapshot.project.dependencies.contains(*provider))
}

pub(crate) fn emit(
    model: &jails_model::AppModel,
    output: &mut RenderedTree,
    observed: &Observed<'_>,
) -> Result<(), CompileError> {
    emit_capability::lower_and_emit(model, output, observed)?;
    emit_java::lower_and_emit(model, output, observed)?;
    emit_operation::lower_and_emit(model, output)?;
    crate::emit_relation::lower_and_emit(model, output)?;
    emit_operation::outbox::lower_and_emit(model, output)?;
    emit_component::lower_and_emit(model, output)?;
    emit_http::lower_and_emit(model, output, observed.spring_boot)?;
    crate::emit_architecture::lower(model, output)?;
    package_infos(output, observed.jspecify)?;
    tidy_java(output);
    Ok(())
}

/// Collapse runs of blank lines in every emitted Java file.
///
/// **One rule in one place, for the reason the legacy write path had the same
/// one.** palantir-java-format removes a doubled blank line, so leaving one in
/// means `add format` -- which jails installs itself -- fails `jails check` on
/// a project whose every line jails wrote. Templates produce them without
/// meaning to: `api_exception_handler_java.java` has a `{{duplicate_key_import}}`
/// line that substitutes to nothing on a project without the JDBC starter, and
/// the empty line it leaves behind is invisible in the template and obvious in
/// the output.
///
/// A blank line inside a text block is data and is left alone, which is why
/// this counts `"""` fences rather than trimming unconditionally.
/// One `package-info.java` for every package this compile writes main Java
/// into.
///
/// **Emitted here rather than per generator**, for the reason the legacy write
/// path had the same rule: it is a fact about a *package*, and a rule twenty
/// renderers have to remember is a rule that decays the first time somebody
/// adds one. Running over the finished tree also makes "one per package"
/// structural rather than something to check.
///
/// Main sources only. A test package is not part of anyone's API and a
/// nullness checker configured over `src/test` is a choice the reader makes.
fn package_infos(output: &mut RenderedTree, jspecify: bool) -> Result<(), CompileError> {
    if !jspecify {
        return Ok(());
    }
    const ROOT: &str = ".jails/generated/main/java/";
    let packages: std::collections::BTreeSet<String> = output
        .files
        .iter()
        .filter(|(_, file)| file.kind == FileKind::JavaMain)
        .filter_map(|(path, _)| {
            let rest = path.as_str().strip_prefix(ROOT)?;
            let (directory, _) = rest.rsplit_once('/')?;
            Some(directory.to_string())
        })
        .collect();
    for directory in packages {
        let path = ProjectPath::parse(format!("{ROOT}{directory}/package-info.java"))
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
