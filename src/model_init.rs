//! `jails model init`: the one way a project acquires a canonical model.
//!
//! **This is what `model import` became.** Import existed to read the legacy
//! ledger, translate the two declaration kinds it understood, and hand the
//! result to the pre-v1 upgrader -- three things that no longer exist. What it
//! was doing underneath, though, is the thing every project still needs: read
//! the build to learn the package, the Java release, the build system and
//! whether Spring is present, and write one `.jails/model.jdl` stating them.
//!
//! It writes an *empty* model, deliberately. Import inferred `storage` from
//! `Project::sql_dialect()`, which has no "none" answer and returned Postgres
//! for a project with no database at all -- so adopting a project that had
//! never seen a database spliced Flyway, the PostgreSQL driver, three
//! Testcontainers artifacts, a compose service and a datasource URL into it.
//! Storage is a decision, and `jails add db` is where a reader makes it.

use crate::model_generate::{report_plan, write_bundle};
use crate::{Invocation, Output};
use jails_contracts::{BuildSystem, CanonicalModelPatch, ModelFileUpdate, ProjectPath};
use jails_support::{Failure, Result};

pub(crate) fn run(invocation: Invocation) -> Result<()> {
    let root = std::env::current_dir()
        .map_err(|error| Failure::Told(format!("could not read current directory: {error}")))?;
    if std::path::Path::new(crate::model_command::JDL_PATH).is_file() {
        return Err(Failure::Told(format!(
            "`{}` already exists.\n       fix: edit it, or use the generators to evolve it",
            crate::model_command::JDL_PATH
        )));
    }
    let source = draft(&root)?;
    let model = jails_model::parse_jdl(&source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let model_path = std::path::Path::new(crate::model_command::JDL_PATH);
    let snapshot =
        jails_workspace::capture_import(&root, model_path, source.as_bytes(), model, &[])
            .map_err(|error| Failure::Told(format!("could not capture the project: {error}")))?;
    let draft = jails_compiler::Compiler::compile(&snapshot, None)
        .map_err(|error| Failure::Told(format!("could not compile the new model: {error}")))?;
    let patch_bytes = serde_json::to_vec(&serde_json::json!({"kind": "init-model"}))
        .map_err(|error| Failure::Told(format!("could not encode model patch: {error}")))?;
    let bundle = jails_workspace::materialize_with_model(
        &snapshot,
        CanonicalModelPatch {
            schema: "jails.model-patch.v1".to_string(),
            bytes: patch_bytes,
        },
        draft,
        Some(ModelFileUpdate {
            path: ProjectPath::parse(crate::model_command::JDL_PATH).map_err(Failure::Told)?,
            bytes: source.into_bytes(),
        }),
        jails_compiler::COMPILER_VERSION,
    )
    .map_err(|error| Failure::Told(format!("could not materialize the plan: {error}")))?;
    if let Some(path) = &invocation.plan_out {
        write_bundle(path, &bundle)?;
    }
    if invocation.pretend || invocation.plan_out.is_some() {
        return report_plan(&bundle, &invocation);
    }
    let execution = jails_workspace::execute(&root, &bundle)
        .map_err(|error| Failure::Told(format!("could not write the new model: {error}")))?;
    if invocation.output == Output::Human {
        println!(
            "initialized {}: {} files written",
            execution.plan_digest.as_str(),
            execution.files_written
        );
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&execution)
                .map_err(|error| Failure::Told(format!("could not encode execution: {error}")))?
        );
    }
    Ok(())
}

/// The smallest v1 source that states what the build already says.
///
/// Every axis is read rather than assumed, and an axis that cannot be read is
/// refused rather than guessed -- the same bar `gradle.rs` set for build files
/// and for the same reason: a model that quietly says `java 21` about a
/// project targeting 26 produces code the project cannot compile, and nothing
/// reports it.
pub(crate) fn draft(root: &std::path::Path) -> Result<String> {
    let project = jails_project::model::Project::load(root)?;
    let build_system = jails_workspace::observe_build_system(root);
    let build = match build_system {
        BuildSystem::Maven => "maven",
        BuildSystem::Gradle => "gradle",
        BuildSystem::Unknown => {
            return Err(Failure::Told(
                "this directory has no recognised build file.\n       fix: run this in a Maven or Gradle project root"
                    .to_string(),
            ));
        }
    };
    let platform = if jails_workspace::observe_spring_boot(root, build_system).is_some() {
        "spring"
    } else {
        "plain"
    };
    let java = project.java_release().ok_or_else(|| {
        Failure::Told(
            "the project Java release cannot be read from its build.\n       fix: declare a Maven release/source level or a Gradle toolchain, then retry"
                .to_string(),
        )
    })?;
    let directory = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("application");
    let name = jails_model::upper_camel_case(directory);
    let id = crate::model_resource::java_to_label(&name);
    Ok(format!(
        "jdl 1\n\napp {name} @id(project_{id}) {{\n  pkg {}\n  java {java}\n  platform {platform}\n  build {build}\n  storage none\n}}\n",
        project.base(),
    ))
}
