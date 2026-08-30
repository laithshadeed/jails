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

const COMMAND: &str = include_str!("../../../../templates/spring/command_java.java");
const TEST: &str = include_str!("../../../../templates/generate/command_test.java");

pub(super) fn files(model: &AppModel, component: &Component) -> Result<Vec<Emitted>, CompileError> {
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
            TEST.replace("{{pkg}}", &pkg).replace("{{name}}", name),
        )?,
    ])
}
