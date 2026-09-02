//! `component cli <Name>`: a dispatcher for the commands this project has.
//!
//! Plain Java, like `command`, and found by the same shape: a registry of
//! `Command` plus a `return commands;` anchor. That shape is what lets
//! `g command` register itself into either this or the `App.java` a `new-cli`
//! project already has, without either knowing about the other.
//!
//! **The entry-point claim is a decision, and it is made here.** A project
//! with two dispatchers has two `main` methods, and a search of the source
//! picks whichever the walk reaches first — which is how a jar and
//! `jails run` came to start different classes. The POM is Maven's own record
//! of the entry point, so it is the one thing that decides, and it moves only
//! from a stub jails wrote that nobody has used.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_model::{AppModel, Component, ComponentKind, ComponentReference};

const CLI: crate::Template = crate::template!("spring/cli_java.java");
const TEST: crate::Template = crate::template!("spring/cli_test_java.java");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    let name = &component.name;
    let pkg = package(model, Package::Cli);
    let class = format!("{name}Cli");
    Ok(vec![
        java(
            component,
            "cli",
            &pkg,
            &class,
            false,
            true,
            CLI.resolve(templates)?
                .replace("{{pkg}}", &pkg)
                .replace("{{class}}", &class)
                .replace("{{program}}", &name.to_lowercase())
                .replace("{{registrations}}", &registrations(model, component)),
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("{class}Test"),
            true,
            true,
            TEST.resolve(templates)?
                .replace("{{pkg}}", &pkg)
                .replace("{{class}}", &class),
        )?,
    ])
}

/// One `commands.put(...)` per command that named this dispatcher.
///
/// **A dispatcher whose `commands()` is empty answers nothing**, and it says
/// so only by exiting 2 at run time -- which is what a proof application's
/// integration test found after `g command Greet --on LedgerCli` reported
/// success. The legacy engine spliced a line into the file after the fact;
/// here the registry is a projection of the model, so it is rendered rather
/// than edited, and a command removed from the model leaves no line behind.
///
/// Only commands that named *this* dispatcher. A `new-cli` project's
/// `App.java` is a reader file rather than a component, and a command with no
/// `on` is registered there by the splice that has always done it -- claiming
/// it here as well would register it twice.
///
/// `components` is a `BTreeMap`, so the order is the stable-ID order and the
/// same model renders the same file.
fn registrations(model: &AppModel, cli: &Component) -> String {
    model
        .components
        .values()
        .filter(|component| component.kind == ComponentKind::Command)
        .filter(|command| command.on == Some(ComponentReference::Component(cli.id.clone())))
        .map(|command| {
            format!(
                "        commands.put({0}Command.NAME, {0}Command::run);\n",
                command.name
            )
        })
        .collect()
}

/// The entry point this model's `cli` components may claim, if any.
///
/// `None` in four cases, and each is somebody's decision rather than jails':
/// a POM naming no entry point at all is a Spring Boot project, where the
/// plugin finds `@SpringBootApplication` itself; a POM naming anything but the
/// `App` stub is a choice already made; an `App` that registers a command of
/// its own is the project's real CLI, so moving the jar out from under it
/// would break what the reader built; and two `cli` declarations are two
/// candidates, which is a question rather than an answer.
pub(super) fn entry_point(
    snapshot: &jails_contracts::WorkspaceSnapshot,
    model: &jails_model::AppModel,
) -> Option<String> {
    let mut declared = model
        .components
        .values()
        .filter(|component| component.kind == jails_model::ComponentKind::Cli);
    let only = declared.next()?;
    if declared.next().is_some() {
        return None;
    }
    let pom = snapshot
        .files
        .get(&jails_contracts::ProjectPath::parse("pom.xml").ok()?)?;
    let pom = std::str::from_utf8(&pom.bytes).ok()?;
    let current = main_class(pom)?;
    let base = &model.project.base_package;
    if current != format!("{base}.App") {
        return None;
    }
    // The captured `App`, not the directory: `registers something` has to be
    // read off the same snapshot everything else in this plan is read from.
    let app = snapshot.files.iter().find(|(path, _)| {
        path.as_str()
            .ends_with(&format!("{}/App.java", base.replace('.', "/")))
    })?;
    let source = std::str::from_utf8(&app.1.bytes).ok()?;
    if jails_codemod::text::blanked(source).contains("commands.put(") {
        return None;
    }
    Some(format!(
        "{}.{}Cli",
        model.project.package_for(Package::Cli),
        only.name
    ))
}

fn main_class(pom: &str) -> Option<&str> {
    let open = "<mainClass>";
    let start = pom.find(open)? + open.len();
    let end = pom[start..].find("</mainClass>")? + start;
    let value = pom[start..end].trim();
    (!value.is_empty()).then_some(value)
}
