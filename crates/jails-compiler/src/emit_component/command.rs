//! `component command <Name>`: one CLI subcommand.
//!
//! Plain Java with no framework in it, which is why it works in a `new-cli`
//! project as well as a Spring one.
//!
//! **`run` returns an exit code instead of calling `System.exit`**, and takes
//! its output streams as arguments instead of reaching for `System.out`. Both
//! exist so a test can drive the whole command in-process and assert on what
//! it printed, and `main` stays the only place that exits.
//!
//! The registration into the project's dispatcher is a reader-file patch, not
//! something emitted here — see `DocumentIntent::EnsureCommandRegistration`.
//! Hand-pasting a dispatch line after every `g command` is exactly the
//! plumbing this tool exists to remove.

use super::{Emitted, Package, java, package};
use crate::CompileError;
use jails_model::{AppModel, Component};

const COMMAND: crate::Template = crate::template!("spring/command_java.java");
const TEST: crate::Template = crate::template!("generate/command_test.java");

pub(super) fn files(
    model: &AppModel,
    component: &Component,
    templates: &jails_contracts::TemplateOverrides,
) -> Result<Vec<Emitted>, CompileError> {
    let name = &component.name;
    let pkg = package(model, Package::Cli);
    Ok(vec![
        java(
            component,
            "command",
            &pkg,
            &format!("{name}Command"),
            false,
            true,
            COMMAND
                .resolve(templates)?
                .replace("{{pkg}}", &pkg)
                .replace("{{name}}", name)
                .replace("{{word}}", &name.to_lowercase()),
        )?,
        java(
            component,
            "test",
            &pkg,
            &format!("{name}CommandTest"),
            true,
            true,
            TEST.resolve(templates)?
                .replace("{{pkg}}", &pkg)
                .replace("{{name}}", name),
        )?,
    ])
}
