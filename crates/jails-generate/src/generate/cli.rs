//! `generate command` and `generate cli`: the dispatcher a plain-Maven
//! project routes argv through, and the subcommands registered into it.
//!
//! Dispatchers are found by **shape**, not filename -- the registry type and
//! the `return commands;` anchor -- so both `new-cli`s `App.java` and a
//! generated `<Name>Cli.java` qualify. Registration and unregistration are
//! exact inverses: destroying a command that stayed registered leaves the
//! project calling a class that is gone.

// ---- command: a CLI subcommand for `new-cli` projects, which otherwise get
// a Hello World `main` and no pattern for growing past it. ----

// ---- cli: the dispatcher that `generate command` leaves you to write. ----

pub fn cli_java(pkg: &str, class: &str, program: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cli_java.java"),
        // Empty here, and rendered from the model on the canonical path. This
        // engine splices each registration in afterwards with
        // `register_command`, so a freshly written dispatcher has none.
        &[
            ("pkg", pkg),
            ("class", class),
            ("program", program),
            ("registrations", ""),
        ],
    )
}

pub fn cli_test(pkg: &str, class: &str) -> String {
    crate::template::render(
        crate::template_here!("spring/cli_test_java.java"),
        &[("pkg", pkg), ("class", class)],
    )
}

// ---- registering a generated command with the dispatcher ----

pub use jails_java::dispatch::{
    is_dispatcher, registry_body, splice_registration, unsplice_registration,
};

pub use jails_java::java::package_of;

#[cfg(test)]
mod tests {
    use super::*;

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
