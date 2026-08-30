//! `jails model init`: the canonical on-ramp for a project jails did not create.
//!
//! **This is what the legacy engine was load-bearing for.** `new` seeds a
//! model, so a project jails creates is canonical from its first command, and
//! `model import` carries a *legacy ledger* across. Neither reaches the case
//! that matters most: somebody else's repository, which has no model, no
//! ledger, and until now no command that could give it one. Every mutation
//! there went through the legacy path, which is why deleting that path was
//! blocked on a feature rather than on a port.
//!
//! What it writes is the app block and nothing else. The reader's existing
//! Java stays exactly where it is and stays theirs -- this does not adopt a
//! line of it. What changes is that the *next* `jails g` renders into
//! `.jails/generated` through the compiler instead of splicing into `src/`
//! through the engine, which is the whole of the cutover for a foreign
//! project.
//!
//! Every field is read off the project rather than asked for, because each is
//! a fact the project already states and a prompt for a fact is a prompt for a
//! wrong answer. `storage none` is the one judgement, and it is the same one
//! `new` makes: jails has installed no database here, so the model claims
//! none. `add db` says otherwise when the reader means it.

use crate::{Invocation, Output};
use jails_support::{Failure, Result};
use std::path::Path;

pub(crate) fn run(invocation: Invocation) -> Result<()> {
    let root = crate::model_command::root()?;
    let project = jails_project::model::Project::discover()?;
    refuse_if_modelled(&root)?;

    let source = derive(&project)?;
    let model = jails_model::parse_jdl(&source)
        .map_err(|diagnostics| Failure::Told(diagnostics.to_string().trim_end().to_string()))?;
    let model_path = Path::new(crate::model_command::JDL_PATH);
    // Through the one canonical writer, like every other model mutation. The
    // model file is not a special case that may be dropped on disk: the
    // executor locks, rechecks its preconditions and publishes an exact
    // after-image, which is what makes a half-finished `model init` impossible
    // rather than merely unlikely.
    let snapshot =
        jails_workspace::capture_import(&root, model_path, source.as_bytes(), model, &[])
            .map_err(|error| Failure::Told(format!("could not capture this project: {error}")))?;
    let draft = jails_compiler::Compiler::compile(&snapshot, None)
        .map_err(|error| Failure::Told(format!("could not compile the new model: {error}")))?;
    let patch_bytes = serde_json::to_vec(&serde_json::json!({"kind": "init-model"}))
        .map_err(|error| Failure::Told(format!("could not encode init patch: {error}")))?;
    let bundle = jails_workspace::materialize_with_model(
        &snapshot,
        jails_contracts::CanonicalModelPatch {
            schema: "jails.model-patch.v1".to_string(),
            bytes: patch_bytes,
        },
        draft,
        Some(jails_contracts::ModelFileUpdate {
            path: jails_contracts::ProjectPath::parse(crate::model_command::JDL_PATH)
                .map_err(Failure::Told)?,
            bytes: source.into_bytes(),
        }),
        jails_compiler::COMPILER_VERSION,
    )
    .map_err(|error| Failure::Told(format!("could not materialize the new model: {error}")))?;
    if invocation.pretend {
        if invocation.output == Output::Human {
            println!("--pretend: would create {}", crate::model_command::JDL_PATH);
        }
        return Ok(());
    }
    jails_workspace::execute(&root, &bundle).map_err(|error| {
        Failure::Told(format!("could not write the application model: {error}"))
    })?;
    if invocation.output == Output::Human {
        println!("  create  {}", crate::model_command::JDL_PATH);
        println!(
            "This project is canonical now: `jails g` renders through the compiler into \
             `.jails/generated`, and your own sources under `src/` stay yours."
        );
    }
    Ok(())
}

/// One editable source, which is the rule the whole cutover turns on.
fn refuse_if_modelled(root: &Path) -> Result<()> {
    for existing in [
        crate::model_command::JDL_PATH,
        crate::model_command::TOML_PATH,
    ] {
        if root.join(existing).is_file() {
            return Err(Failure::Told(format!(
                "this project already has an application model at `{existing}`.\n       fix: edit it, or run `jails sync`; `model init` is for a project that has none"
            )));
        }
    }
    if root.join(".jails/ledger.toml").is_file() {
        return Err(Failure::Told(
            "this project has a legacy ledger, so its declarations can be carried across rather than discarded.\n       fix: run `jails model import`, which is the one-way door for a project jails already generated into"
                .to_string(),
        ));
    }
    Ok(())
}

/// The app block this project already states.
fn derive(project: &jails_project::model::Project) -> Result<String> {
    let root = project.root();
    let label = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("application");
    let application = java_type_name(&camel_case(label));
    let application = match application.is_empty() {
        true => "Application".to_string(),
        false => application,
    };
    let package = project.base();
    if package.is_empty() {
        return Err(Failure::Told(
            "could not read this project's base package, so the model would name the wrong one.\n       fix: generate from a directory holding a Java source tree"
                .to_string(),
        ));
    }
    let java = project.java_release().ok_or_else(|| {
        Failure::Told(
            "could not read this project's Java release from its build.\n       fix: declare a Maven release/source level or a Gradle toolchain, then retry"
                .to_string(),
        )
    })?;
    let platform = match project.flavor() {
        jails_project::pom::Flavor::SpringBoot => "spring",
        jails_project::pom::Flavor::PlainMaven => "plain",
    };
    // Named rather than defaulted: a model that says `maven` for a Gradle
    // project would render a pom nobody builds with.
    let build = match jails_spec::build::detect(root) {
        jails_spec::build::Build::Maven => "maven",
        jails_spec::build::Build::Gradle => "gradle",
        other => {
            return Err(Failure::Told(format!(
                "jails cannot model a `{other:?}` build.\n       fix: `model init` supports Maven and Groovy Gradle; run `jails modernize` first, or model this project by hand"
            )));
        }
    };
    Ok(format!(
        "jdl 1\n\napp {application} {{\n  pkg {package}\n  java {java}\n  \
         platform {platform}\n  build {build}\n  storage none\n}}\n"
    ))
}

fn camel_case(name: &str) -> String {
    let mut out = String::new();
    let mut uppercase = true;
    for character in name.chars() {
        match character.is_ascii_alphanumeric() {
            true if uppercase => {
                out.extend(character.to_uppercase());
                uppercase = false;
            }
            true => out.push(character),
            false => uppercase = true,
        }
    }
    out
}

fn java_type_name(name: &str) -> String {
    let mut characters = name.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => String::new(),
    }
}
