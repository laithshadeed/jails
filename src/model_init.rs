//! `jails model init`: the canonical on-ramp for a project jails did not create.
//!
//! `new` seeds a model, so a project jails creates is canonical from its
//! first command; this is the command that gives somebody else's repository
//! one.
//!
//! What it writes is the app block and nothing else. The reader's existing
//! Java stays exactly where it is and stays theirs -- this does not adopt a
//! line of it. What changes is that the *next* `jails g` renders into
//! `.jails/generated` through the compiler.
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
    run_as(invocation)
}

fn run_as(invocation: Invocation) -> Result<()> {
    let root = invocation.root()?;
    let project = jails_project::model::Project::load(&root)?;
    let source = derive(&project)?;
    // **Deriving the same model twice is a no-op, not a collision.** Every
    // other canonical frontend is idempotent -- a second `g record` with the
    // same shape writes nothing -- and so is this. Compared by content: a
    // `.jails/model.jdl` that differs is the one editable source, and still
    // refuses.
    if let Ok(existing) = std::fs::read_to_string(root.join(crate::model_command::JDL_PATH))
        && existing == source
    {
        if invocation.output == Output::Human {
            println!("  exists  {}", crate::model_command::JDL_PATH);
        }
        return Ok(());
    }
    refuse_if_modelled(&root)?;

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
    let draft = jails_compiler::Compiler::compile(
        &snapshot,
        &snapshot.model.model,
        &jails_model::Evolution::none(),
    )
    .map_err(|error| Failure::Told(format!("could not compile the new model: {error}")))?;
    let bundle = jails_workspace::materialize(
        &snapshot,
        jails_contracts::PlanInput::init_model(),
        draft,
        Some(jails_contracts::ModelFileUpdate {
            retire: Vec::new(),
            path: jails_contracts::ProjectPath::parse(crate::model_command::JDL_PATH)
                .map_err(Failure::Told)?,
            bytes: source.into_bytes(),
        }),
        jails_compiler::COMPILER_VERSION,
        jails_workspace::Restore::Refuse,
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
             `.jails/generated`, and your own sources under `src/` stay yours.",
        );
    }
    Ok(())
}

/// One editable source: a project that has a model is refused by name.
pub(crate) fn refuse_if_modelled(root: &Path) -> Result<()> {
    let existing = crate::model_command::JDL_PATH;
    if root.join(existing).is_file() {
        return Err(Failure::Told(format!(
            "this project already has an application model at `{existing}`.\n       fix: edit it, or run `jails sync`; `model init` is for a project that has none"
        )));
    }
    // **A project holding `.jails/ledger.toml` is refused by name.** Nothing
    // in this binary can read or write one, so the honest answer names the
    // file rather than seeding a model beside declarations this jails cannot
    // see -- which would strand a project's whole contents outside the model
    // that owns it.
    if root.join(".jails/ledger.toml").is_file() {
        return Err(Failure::Told(
            "this project has a legacy ledger, which this jails cannot read.\n       fix: remove `.jails/ledger.toml` and its `.jails/objects`, or keep using the jails that wrote them"
                .to_string(),
        ));
    }
    Ok(())
}

/// The app block this project already states.
pub(crate) fn derive(project: &jails_project::model::Project) -> Result<String> {
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
    // **A build that states no release is read as targeting the floor.** An
    // ordinary Gradle script declares neither a toolchain nor a
    // `sourceCompatibility` and compiles with whatever JDK Gradle is running
    // on, so there is no configured release to keep, and refusing would put
    // `jails g` out of reach of the commonest build file there is. The floor
    // is the safe reading: generated code compiles on every release jails
    // supports, and a project that *does* state one keeps it, because
    // generation must never rewrite an adopted release merely because jails'
    // own default advanced.
    let java = project
        .java_release()
        .unwrap_or(u32::from(jails_model::JAVA_RELEASE_FLOOR));
    let platform = match project.is_spring_boot() {
        true => "spring",
        false => "plain",
    };
    // Named rather than defaulted: a model that says `maven` for a Gradle
    // project would render a pom nobody builds with.
    let build = match jails_spec::build::detect(root) {
        jails_spec::build::Build::Maven => "maven",
        jails_spec::build::Build::Gradle => "gradle",
        // **Named the way the reader would name it, not the way the enum
        // spells it.** `Foreign("Gradle")` is a multi-module root or a Kotlin
        // script -- a build jails recognises by filename and will not read --
        // and the fact that matters is the one such a root never states: which
        // Java release the code jails writes has to compile against. A
        // multi-module build declares it per module, so there is no answer
        // here and no defensible default; jails' own target on a project whose
        // modules build with 17 is code that does not compile.
        jails_spec::build::Build::Foreign(name) => {
            return Err(Failure::Told(format!(
                "this project is built by {name}, and jails cannot read its Java release from here.\n       fix: run `jails g` inside a module whose build declares a Gradle toolchain, or model this project by hand"
            )));
        }
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
