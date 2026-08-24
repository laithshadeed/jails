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
    let word = name.to_lowercase();
    format!(
        r#"package {pkg};

import java.io.PrintStream;

/**
 * The {{@code {word}}} subcommand.
 *
 * <p>{{@link #run}} returns an exit code instead of calling
 * {{@code System.exit}}, and takes its output streams as arguments instead of
 * reaching for {{@code System.out}}. Both exist so a test can drive the whole
 * command in-process and assert on what it printed. Keep {{@code main}} the
 * only place that exits.
 *
 * <p>jails registered this in the project's dispatcher when it generated the
 * class, so {{@code {word}}} already works. If you need to do it by hand -- a
 * second dispatcher, or one jails could not find -- the line is:
 *
 * <pre>{{@code
 * commands.put({name}Command.NAME, {name}Command::run);
 * }}</pre>
 */
public final class {name}Command {{

    /** The word that selects this command on the command line. */
    public static final String NAME = "{word}";

    public static final String USAGE = "usage: {word} <argument>";

    /** Conventional exit code for "you invoked this wrong". */
    public static final int USAGE_ERROR = 2;

    private {name}Command() {{}}

    /** Runs the command, returning the exit code the process should end with. */
    public static int run(PrintStream out, PrintStream err, String... args) {{
        if (args.length != 1) {{
            err.println(USAGE);
            return USAGE_ERROR;
        }}

        out.println(args[0]);
        return 0;
    }}
}}
"#
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
    format!(
        r#"package {pkg};

import java.io.PrintStream;
import java.util.LinkedHashMap;
import java.util.SequencedMap;

/**
 * Argv dispatch for the {program} command line: it owns argument routing, exit
 * codes and streams, and nothing else.
 *
 * <p>The registry is a parameter of {{@link #run}}, not a static the method
 * reaches for. That is what lets a test drive the whole dispatcher with its own
 * commands, without a real one existing and without touching
 * {{@code System.out}}. {{@link #commands()}} is the one place to edit as you add
 * commands; {{@code main}} is the only place that exits.
 *
 * {{@snippet :
 * var out = new ByteArrayOutputStream();
 * int code = {class}.run({class}.commands(), new PrintStream(out), System.err, "greet", "world");
 * }}
 */
public final class {class} {{

    /**
     * One subcommand. Matches the shape {{@code jails generate command}} emits,
     * so {{@code SomethingCommand::run}} is a method reference away.
     */
    @FunctionalInterface
    public interface Command {{
        int run(PrintStream out, PrintStream err, String... args);
    }}

    /** Conventional exit code for "you invoked this wrong". */
    public static final int USAGE_ERROR = 2;

    private {class}() {{}}

    /**
     * The commands this CLI answers to, in the order they should be listed.
     *
     * <p>Add yours here -- a {{@code SequencedMap}} because help output that
     * reorders itself between runs is a diff nobody wants:
     *
     * {{@snippet :
     * commands.put(ImportCommand.NAME, ImportCommand::run);
     * }}
     */
    public static SequencedMap<String, Command> commands() {{
        var commands = new LinkedHashMap<String, Command>();
        return commands;
    }}

    /** Runs one invocation and returns the exit code the process should end with. */
    public static int run(SequencedMap<String, Command> commands, PrintStream out, PrintStream err, String... args) {{
        var name = args.length == 0 ? "help" : args[0];

        if (name.equals("help") || name.equals("--help") || name.equals("-h")) {{
            usage(commands, out);
            return 0;
        }}

        var command = commands.get(name);
        if (command == null) {{
            err.println("unknown command: " + name);
            usage(commands, err);
            return USAGE_ERROR;
        }}

        // Everything after the command word belongs to the command itself.
        var rest = new String[args.length - 1];
        System.arraycopy(args, 1, rest, 0, rest.length);
        return command.run(out, err, rest);
    }}

    private static void usage(SequencedMap<String, Command> commands, PrintStream to) {{
        to.println("usage: {program} <command> [args]");
        to.println();
        to.println("commands:");
        to.println("  help");
        commands.keySet().forEach(name -> to.println("  " + name));
    }}

    public static void main(String[] args) {{
        System.exit(run(commands(), System.out, System.err, args));
    }}
}}
"#,
        program = program,
    )
}

pub fn cli_test(pkg: &str, class: &str) -> String {
    format!(
        r#"package {pkg};

import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.util.LinkedHashMap;
import java.util.SequencedMap;

import static org.assertj.core.api.Assertions.assertThat;

class {class}Test {{

    private final ByteArrayOutputStream out = new ByteArrayOutputStream();
    private final ByteArrayOutputStream err = new ByteArrayOutputStream();

    /**
     * A registry of test doubles. Because {{@code run}} takes the commands as an
     * argument, the dispatcher is testable on its own -- these assertions hold
     * before a single real command exists.
     */
    private SequencedMap<String, {class}.Command> commands() {{
        var commands = new LinkedHashMap<String, {class}.Command>();
        commands.put("echo", (out, err, args) -> {{
            out.println(String.join(" ", args));
            return 0;
        }});
        commands.put("boom", (out, err, args) -> {{
            err.println("failed");
            return 1;
        }});
        return commands;
    }}

    private int run(String... args) {{
        return {class}.run(commands(), new PrintStream(out), new PrintStream(err), args);
    }}

    @Test
    void routesToTheNamedCommandAndPassesTheRestOfArgv() {{
        assertThat(run("echo", "hello", "world")).isZero();
        assertThat(out.toString()).contains("hello world");
    }}

    @Test
    void returnsWhateverTheCommandReturned() {{
        assertThat(run("boom")).isEqualTo(1);
        assertThat(err.toString()).contains("failed");
    }}

    @Test
    void listsEveryCommandInHelp() {{
        assertThat(run("help")).isZero();
        assertThat(out.toString()).contains("echo").contains("boom");
    }}

    @Test
    void treatsNoArgumentsAsHelpRatherThanAnError() {{
        assertThat(run()).isZero();
        assertThat(out.toString()).contains("usage:");
    }}

    @Test
    void namesTheUnknownCommandAndExitsTwo() {{
        assertThat(run("nope")).isEqualTo({class}.USAGE_ERROR);
        assertThat(err.toString()).contains("nope");
    }}

    /** Help ordering is part of the contract, hence SequencedMap. */
    @Test
    void listsCommandsInRegistrationOrder() {{
        run("help");
        var text = out.toString();
        assertThat(text.indexOf("echo")).isLessThan(text.indexOf("boom"));
    }}
}}
"#
    )
}

// ---- registering a generated command with the dispatcher ----

/// Splice `commands.put(FooCommand.NAME, FooCommand::run);` into the
/// project's `*Cli.java`.
///
/// jails' rule used to be that only `pom.rs` edits a file the user owns, and
/// so `generate command` merely *documented* the dispatch line for you to
/// paste. But that rule was always a proxy for the real one -- an edit must be
/// surgical and leave every other byte alone -- and pasting a line by hand
/// after every single `generate` is exactly the plumbing this tool exists to
/// remove. The splice is idempotent and touches one line inside one method.
///
/// No dispatcher means jails cannot know where it goes: it says so and leaves
/// the Javadoc instructions as the fallback.
///
/// **More than one is the normal case, not an exotic one.** `new-cli` writes
/// `App.java`, and the obvious next command -- `g cli Ledger` -- writes a
/// second dispatcher, so any project with its own CLI has two. Guessing
/// between them would produce a command wired into the wrong entry point, so
/// `--on <Dispatcher>` names it: `jails g command Reconcile --on Ledger`,
/// which is also what `strategy_on` carries in a manifest. Without it the
/// note names every candidate, since "add it to the one you meant" without
/// saying which ones exist is a refusal that teaches nothing (plan.md §9.6).
pub(super) fn register_command(
    root: &Path,
    base: &str,
    name: &str,
    into: Option<&str>,
) -> Result<()> {
    let dispatchers = find_dispatchers(&root.join("src/main/java"));
    let chosen;
    let dispatcher = match (dispatchers.as_slice(), into) {
        ([], _) => {
            println!(
                "note: no *Cli.java dispatcher found -- see {name}Command's Javadoc for the dispatch line,\n      \
                 or run `jails generate cli <Name>` to get one that registers commands for you"
            );
            return Ok(());
        }
        (_, Some(wanted)) => {
            let Some(found) = dispatchers
                .iter()
                .find(|path| matches_dispatcher(path, wanted))
            else {
                return Err(format!(
                    "--on {wanted} does not name a dispatcher in this project.\n       \
                     fix: use one of {}, or `jails generate cli {wanted}` to create it",
                    dispatcher_names(&dispatchers).join(", ")
                ));
            };
            chosen = found.clone();
            &chosen
        }
        ([one], None) => one,
        (many, None) => {
            println!(
                "note: {name}Command was not registered -- this project has {} dispatchers ({}).\n      \
                 fix: rerun with `--on <Dispatcher>`, or add the line from {name}Command's Javadoc by hand",
                many.len(),
                dispatcher_names(many).join(", ")
            );
            return Ok(());
        }
    };

    let source = fs::read_to_string(dispatcher)
        .map_err(|e| format!("failed to read {}: {e}", dispatcher.display()))?;
    let command_class = format!("{name}Command");
    // Scoped to the registry body, not the whole file: the dispatcher's own
    // Javadoc shows an example `commands.put(...)` line, and a whole-file
    // `contains` matched *that* -- so generating a command with the same name
    // as the example silently skipped registration.
    if registry_body(&source).is_some_and(|body| body.contains(&format!("{command_class}::run"))) {
        println!(
            "  exists  {command_class} is already registered in {}",
            dispatcher.display()
        );
        return Ok(());
    }

    // The dispatcher and the command can be in different packages once
    // `--package` is involved, so the registration may need an import too.
    let dispatcher_pkg = package_of(&source).unwrap_or_else(|| base.to_string());
    let command_pkg = subpackage(base, layout::CLI);

    let Some(spliced) = splice_registration(
        &source,
        &command_class,
        &import_of(&dispatcher_pkg, &command_pkg, &command_class),
    ) else {
        println!(
            "note: could not find the `return commands;` line in {} -- add {command_class} by hand",
            dispatcher.display()
        );
        return Ok(());
    };

    crate::apply::put(dispatcher, spliced)?;
    println!("registered {command_class} in {}", dispatcher.display());
    Ok(())
}

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

fn dispatcher_names(dispatchers: &[PathBuf]) -> Vec<String> {
    dispatchers
        .iter()
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()))
        .map(|s| s.to_string())
        .collect()
}

/// Every dispatcher under the source root.
///
/// Recognised by shape, not by filename: `new-cli` writes one called
/// `App.java` and `generate cli` writes one called `<Name>Cli.java`, and both
/// have to be findable. A file merely *named* like one is not enough to edit.
pub(super) fn find_dispatchers(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "java")
                && fs::read_to_string(&path)
                    .map(|s| is_dispatcher(&s))
                    .unwrap_or(false)
            {
                found.push(path);
            }
        }
    }
    found.sort();
    found
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

/// Undo `register_command`, so `destroy command` is its true inverse.
///
/// Without this, destroying a command deleted the class and left the
/// dispatcher calling it -- the project then stops compiling, on the one
/// operation whose whole job is to leave no trace.
pub(super) fn unregister_command(root: &Path, name: &str) -> Result<()> {
    let command_class = format!("{name}Command");
    for dispatcher in find_dispatchers(&root.join("src/main/java")) {
        let source = fs::read_to_string(&dispatcher)
            .map_err(|e| format!("failed to read {}: {e}", dispatcher.display()))?;
        let Some(unspliced) = unsplice_registration(&source, &command_class) else {
            continue;
        };
        crate::apply::put(&dispatcher, unspliced)?;
        println!("unregistered {command_class} from {}", dispatcher.display());
    }
    Ok(())
}

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
    let current = crate::pom::main_class(project.pom())?;
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

pub(super) fn adopt_as_entry_point(project: &Project, cli_package: &str, name: &str) -> Result<()> {
    let pom_path = project.root().join("pom.xml");
    let Ok(pom) = fs::read_to_string(&pom_path) else {
        return Ok(());
    };
    let Some(current) = crate::pom::main_class(&pom) else {
        // No declared entry point: a Spring Boot project, where the plugin
        // finds `@SpringBootApplication` itself.
        return Ok(());
    };
    let stub = format!("{}.App", project.base());
    if current != stub {
        return Ok(());
    }
    let app = crate::generate::main_dir(project.root(), project.base()).join("App.java");
    let registers_something = fs::read_to_string(&app)
        .map(|source| jails_java::java::blanked(&source).contains("commands.put("))
        .unwrap_or(true);
    if registers_something {
        return Ok(());
    }
    let fqcn = if cli_package.is_empty() {
        format!("{name}Cli")
    } else {
        format!("{cli_package}.{name}Cli")
    };
    let Some(updated) = crate::pom::with_main_class(&pom, &fqcn) else {
        return Ok(());
    };
    crate::apply::put_named(&pom_path, &updated, "pom.xml")?;
    println!("  main    {fqcn} is now what the packaged jar starts");
    Ok(())
}
