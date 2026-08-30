//! `generate command` and `generate cli`: the dispatcher a plain-Maven
//! project routes argv through, and the subcommands registered into it.
//!
//! Dispatchers are found by **shape**, not filename -- the registry type and
//! the `return commands;` anchor -- so both `new-cli`s `App.java` and a
//! generated `<Name>Cli.java` qualify. Registration and unregistration are
//! exact inverses: destroying a command that stayed registered leaves the
//! project calling a class that is gone.

use super::*;

// ---- command: a CLI subcommand for `new-cli` projects, which otherwise get
// a Hello World `main` and no pattern for growing past it. ----

pub(super) fn command_java(pkg: &str, name: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/command_java.java"),
        &[("pkg", pkg), ("name", name), ("word", &name.to_lowercase())],
    )
}

pub(super) fn command_test(pkg: &str, name: &str) -> String {
    crate::template::render(
        crate::template_here!("generate/command_test.java"),
        &[("pkg", pkg), ("name", name)],
    )
}

// ---- cli: the dispatcher that `generate command` leaves you to write. ----

pub fn cli_java(pkg: &str, class: &str, program: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cli_java.java"),
        &[("pkg", pkg), ("class", class), ("program", program)],
    )
}

pub fn cli_test(pkg: &str, class: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cli_test_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

// ---- registering a generated command with the dispatcher ----

/// Does this dispatcher answer to `wanted`? `Ledger`, `LedgerCli` and
/// `App` all name a file, and a reader will type whichever they are
/// thinking of.
fn matches_dispatcher(path: &Path, wanted: &str) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    stem.eq_ignore_ascii_case(wanted)
        || stem.eq_ignore_ascii_case(&format!("{wanted}Cli"))
        || stem.eq_ignore_ascii_case(&crate::generate::capitalize(wanted))
        || stem.eq_ignore_ascii_case(&format!("{}Cli", crate::generate::capitalize(wanted)))
}

pub use jails_java::dispatch::{
    is_dispatcher, registry_body, splice_registration, unsplice_registration,
};

/// The dispatcher this command registers itself in, as a plan states it.
///
/// The planning half of [`register_command`], and it looks through the
/// projection rather than at disk: in an aggregate apply the `g cli` row that
/// creates the dispatcher and the `g command` row that registers into it are
/// one transition, so the file the second needs has not been written when the
/// second plans.
///
/// `None` on both the no-dispatcher and the ambiguous case, which is exactly
/// where `register_command` declines too. Neither is silent: the generated
/// command's Javadoc carries the line to add by hand, and `--on <Dispatcher>`
/// is how a project with two of them says which.
pub(super) fn planned_registration(
    project: &Project,
    name: &str,
    into: Option<&str>,
) -> Option<crate::model::CommandRegistration> {
    // A map, for the reason abstract.md §4.1 gives: a positional pair of a
    // path and its text is the fourth shape of "a file", and it compiles when
    // you swap the halves.
    let dispatchers: std::collections::BTreeMap<std::path::PathBuf, String> = project
        .projected_main_sources()
        .into_iter()
        .filter(|(_, text)| is_dispatcher(text))
        .collect();
    let (path, source) = match (dispatchers.len(), into) {
        (0, _) => return None,
        (_, Some(wanted)) => dispatchers
            .iter()
            .find(|(path, _)| matches_dispatcher(path, wanted))?,
        (1, None) => dispatchers.iter().next()?,
        (_, None) => return None,
    };
    let stem = path.file_stem()?.to_str()?;
    let command = format!("{name}Command");
    crate::model::CommandRegistration::parse(
        &qualified(&package_of(source).unwrap_or_default(), stem),
        &qualified(&subpackage(project.base(), layout::CLI), &command),
    )
    .ok()
}

fn qualified(package: &str, name: &str) -> String {
    match package.is_empty() {
        true => name.to_string(),
        false => format!("{package}.{name}"),
    }
}
pub use jails_java::java::package_of;

/// Point the packaged jar at a dispatcher that supersedes jails' own stub.
///
/// `new-cli` writes `App.java` -- a real dispatcher, not a Hello World -- and
/// names it as the jar's `<mainClass>`. `generate cli Ledger` then writes a
/// *second* dispatcher, and the project has two `main` methods with the jar
/// still starting the first. A manifest that generated `LedgerCli` and
/// registered `reconcile` into it produced a jar answering only `help`, and
/// `jails run -- reconcile` said "unknown command".
///
/// So the entry point moves, but only from a stub jails wrote and nobody has
/// used: `App.java` still registering no command of its own. Once a command
/// is registered there, `App` is the project's real CLI and moving the entry
/// point out from under it would break what the reader built. A `<mainClass>`
/// pointing anywhere else is somebody's decision and is left alone.
/// The entry point this `g cli` intends to claim, if it may claim one.
///
/// The same decision [`adopt_as_entry_point`] made, stated rather than
/// performed: V1 wrote the POM itself after the plan, so the routes never knew
/// the packaged jar had moved and nothing recorded it. Here it is one field on
/// the `Change`, and the protocol turns it into a claim the reader can see and
/// `destroy` can put back.
///
/// `None` in three cases, and each is somebody's decision rather than jails':
/// a POM naming no entry point at all is a Spring Boot project, where the
/// plugin finds `@SpringBootApplication` itself; a POM naming anything but the
/// `App` stub is a choice already made; and an `App` that registers a command
/// of its own is the project's real CLI, so moving the jar out from under it
/// would break what the reader built.
pub(super) fn planned_entry_point(
    project: &Project,
    cli_package: &str,
    name: &str,
) -> Option<String> {
    let current = project.main_class()?;
    if current != qualified(project.base(), "App") {
        return None;
    }
    // The projection, not the directory: in an aggregate apply the `App` this
    // has to read may have been written by an earlier intent of the same
    // transition.
    let registers_something = project
        .projected_main_sources()
        .into_iter()
        .find(|(path, source)| {
            path.file_stem().and_then(|stem| stem.to_str()) == Some("App")
                && package_of(source).unwrap_or_default() == project.base()
        })
        .map(|(_, source)| jails_java::java::blanked(&source).contains("commands.put("))
        // No readable `App` at all. V1 declined here too, and for the reason
        // that survives: a stub jails cannot see is not one it can call unused.
        .unwrap_or(true);
    if registers_something {
        return None;
    }
    Some(qualified(cli_package, &format!("{name}Cli")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_java_returns_an_exit_code_and_never_exits_the_process() {
        let src = command_java("com.example.demo", "Greet");

        assert!(src.contains("public final class GreetCommand"));
        assert!(src.contains(r#"public static final String NAME = "greet";"#));
        assert!(
            src.contains("public static int run(PrintStream out, PrintStream err, String... args)")
        );
        // A CLI command has no business depending on Spring.
        assert!(!src.contains("org.springframework"));

        // The whole point: main owns the exit, so the command stays testable
        // in-process, and output goes to injected streams, not System.out.
        // Only the class body is checked -- the Javadoc deliberately shows a
        // `main` that does call System.exit, since that is where it belongs.
        let body = &src[src.find("public final class").unwrap()..];
        assert!(
            !body.contains("System.exit"),
            "run() must not exit the process"
        );
        assert!(
            !body.contains("System.out"),
            "output should go to the injected stream"
        );
    }

    #[test]
    fn command_test_drives_the_command_through_captured_streams() {
        let test = command_test("com.example.demo", "Greet");

        assert!(test.contains("class GreetCommandTest"));
        assert!(test.contains("ByteArrayOutputStream"));
        assert!(
            test.contains("GreetCommand.run(new PrintStream(out), new PrintStream(err), args)")
        );
        assert!(test.contains("GreetCommand.USAGE_ERROR"));
    }

    /// The shape `is_dispatcher` looks for, which is what `new-cli` writes.
    fn dispatcher_java() -> &'static str {
        "package com.example.demo;\n\
         \n\
         import java.util.LinkedHashMap;\n\
         import java.util.SequencedMap;\n\
         \n\
         public class App {\n\
         \x20   static SequencedMap<String, Command> commands() {\n\
         \x20       SequencedMap<String, Command> commands = new LinkedHashMap<>();\n\
         \x20       return commands;\n\
         \x20   }\n\
         }\n"
    }

    /// The dispatcher's own Javadoc carries an example `commands.put(...)`
    /// line. Unregistering must not reach into it -- that is documentation,
    /// not a registration.
    #[test]
    fn unsplice_registration_leaves_an_unregistered_command_alone() {
        let source = dispatcher_java();
        assert!(unsplice_registration(source, "GreetCommand").is_none());
    }
}
