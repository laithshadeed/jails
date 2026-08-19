mod add;
mod compose;
mod console;
mod doctor;
mod generate;
mod inspect;
mod java;
mod new;
mod pom;
mod rename;
mod project;
mod run;
mod sql;
mod why;

use add::Capability;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use compose::Runtime;
use generate::ArtifactKind;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, String>;

/// Prints the program, args and working directory of a command about to be
/// run, for `--debug`. Called right before every `.status()`/`.spawn()` in
/// run.rs/new.rs.
pub(crate) fn debug_cmd(cmd: &std::process::Command) {
    let program = cmd.get_program().to_string_lossy();
    let args: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect();
    let dir = cmd
        .get_current_dir()
        .map(|d| format!("  (in {})", d.display()))
        .unwrap_or_default();
    eprintln!("+ {program} {}{dir}", args.join(" "));
}

/// All unit tests across the crate's modules share one test binary and thus
/// one process-global current directory. Tests that need to change it (to
/// exercise cwd-relative project lookup) must hold this lock for the
/// duration so they can't interleave and race each other.
#[cfg(test)]
pub(crate) static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Parser)]
#[command(
    name = "jails",
    version,
    about = "A rails-CLI-inspired tool for Spring Boot / plain Maven projects"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Print the mvnw/mvnd/mvn/java/git/curl commands jails executes
    #[arg(long, global = true)]
    debug: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Describe the current Maven workspace and active module
    #[command(visible_alias = "info")]
    About {
        /// Emit a stable machine-readable project description
        #[arg(long)]
        json: bool,
    },
    /// Create a new Spring Boot project via start.spring.io
    New {
        name: String,
        #[arg(long, default_value = "web")]
        deps: String,
        #[arg(long, default_value = pom::TARGET_RELEASE)]
        java: String,
        /// Skip `git init` and the .gitignore it normally sets up
        #[arg(long)]
        no_git: bool,
        /// Skip adding spring-boot-devtools (needed for `run --watch`)
        #[arg(long)]
        no_devtools: bool,
    },
    /// Create a new plain Maven CLI project
    NewCli {
        name: String,
        /// Java release to compile against (>= 21)
        #[arg(long, default_value = pom::TARGET_RELEASE)]
        release: String,
        /// Skip `git init` and the .gitignore it normally sets up
        #[arg(long)]
        no_git: bool,
    },
    /// Generate a scaffold or one small Java/SQL artifact
    #[command(visible_alias = "g")]
    Generate {
        kind: ArtifactKind,
        name: String,
        fields: Vec<String>,
        /// Subpackage to place the generated code in, relative to the base
        /// package -- overrides the conventional one for the kind. Pass an
        /// empty string to write straight into the base package.
        #[arg(long)]
        package: Option<String>,
    },
    /// Add one or more capabilities to an existing project: dependencies, code and tests
    #[command(visible_alias = "a")]
    Add {
        #[arg(required = true, num_args = 1..)]
        capabilities: Vec<Capability>,
        /// Base name for the generated class (default: the capability's own)
        #[arg(long)]
        name: Option<String>,
        /// Print what would change without touching the project
        #[arg(long)]
        dry_run: bool,
        /// Write compose.yaml but do not run `docker compose up`
        #[arg(long)]
        no_start: bool,
        /// Subpackage to place the generated code in, relative to the base
        /// package -- overrides the conventional one for the kind. Pass an
        /// empty string to write straight into the base package.
        #[arg(long)]
        package: Option<String>,
    },
    /// Remove what a matching add call would have created
    #[command(visible_alias = "rm")]
    Remove {
        #[arg(required = true, num_args = 1..)]
        capabilities: Vec<Capability>,
        /// Base name for the generated class (default: the capability's own)
        #[arg(long)]
        name: Option<String>,
        /// Print what would change without touching the project
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation prompt
        #[arg(long)]
        force: bool,
        /// Subpackage the generated code was placed in, relative to the base
        /// package -- must match the `--package` passed to `add`.
        #[arg(long)]
        package: Option<String>,
    },
    /// Start compose services (`docker compose up -d`). No args starts all.
    Start {
        #[arg(num_args = 0..)]
        services: Vec<Runtime>,
    },
    /// Stop compose services. No args stops all.
    Stop {
        #[arg(num_args = 0..)]
        services: Vec<Runtime>,
    },
    /// Check everything that has to be true before the app can start
    Doctor,
    /// Explain a failure: pass a log file, pipe one in, or run it bare to start the app
    Why {
        /// A file holding the failure output. Omit to read stdin, or to
        /// start the app and read what it prints.
        log: Option<PathBuf>,
    },
    /// List the HTTP routes this project's source declares
    Routes {
        /// Emit machine-readable output for editor integrations
        #[arg(long)]
        json: bool,
    },
    /// List the Spring beans this project's source registers, and what they inject
    Beans {
        /// Only show beans whose type or stereotype contains this text
        pattern: Option<String>,
        /// Emit machine-readable output for editor integrations
        #[arg(long)]
        json: bool,
    },
    /// Open a database client (`psql` against compose postgres, or sqlite3)
    #[command(visible_alias = "dbconsole")]
    Db {
        /// A SQLite file; omit this to use the compose postgres from `add db`
        file: Option<PathBuf>,
        /// Do not `docker compose up` postgres first
        #[arg(long)]
        no_start: bool,
        /// Extra arguments forwarded to psql/sqlite3
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// jshell with the project's classpath (not a Spring-booted REPL)
    #[command(visible_alias = "c")]
    Console {
        /// Skip `mvn compile` -- use whatever is already in target/
        #[arg(long)]
        no_build: bool,
        /// Extra arguments forwarded to jshell
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Rename a type and every reference to it (files, companions, call sites)
    Rename {
        /// The type's current simple name
        old: String,
        /// The name to give it
        new: String,
        /// Print the plan without touching anything
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation prompt
        #[arg(long)]
        force: bool,
    },
    /// Delete the file(s) a matching generate call would have created
    #[command(visible_alias = "d")]
    Destroy {
        kind: ArtifactKind,
        name: String,
        #[arg(long)]
        force: bool,
        /// Subpackage to place the generated code in, relative to the base
        /// package -- overrides the conventional one for the kind. Pass an
        /// empty string to write straight into the base package.
        #[arg(long)]
        package: Option<String>,
    },
    /// Run tests; bare names become *Test and *IT names use Failsafe
    Test { filter: Option<String> },
    /// Build the project (mvn package)
    Build,
    /// Delete Maven's `target/` directory (`mvn clean`)
    Clean,
    /// Reformat every source file in place (needs `jails add format`)
    Fmt,
    /// Format check + compile + tests (`mvn clean verify`)
    Check,
    /// Pass uncommon arguments through to the project's Maven wrapper
    Mvn {
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Find, compile and run the project's main class
    Run {
        /// Skip compiling/building -- run whatever's already in target/
        #[arg(long)]
        no_build: bool,
        /// Recompile on source changes and keep the app running (Spring
        /// Boot + spring-boot-devtools only -- devtools restarts itself
        /// once target/classes changes)
        #[arg(long)]
        watch: bool,
        /// Everything after `--` is forwarded to the program itself:
        /// `jails run -- normalise input.json`. `last` rather than
        /// `trailing_var_arg` (clap rejects both together) so that a forwarded
        /// `--help` reaches the program instead of being eaten by jails.
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Print a shell-completion script: source <(jails completion bash)
    Completion { shell: Shell },
}

fn main() {
    let cli = Cli::parse();
    let debug = cli.debug;

    let result = match cli.command {
        Command::About { json } => project::about(json),
        Command::New {
            name,
            deps,
            java,
            no_git,
            no_devtools,
        } => new::new(&name, &deps, &java, !no_git, !no_devtools, debug),
        Command::NewCli {
            name,
            release,
            no_git,
        } => new::new_cli(&name, &release, !no_git, debug),
        Command::Generate {
            kind,
            name,
            fields,
            package,
        } => generate::generate(kind, &name, &fields, package.as_deref()),
        Command::Add {
            capabilities,
            name,
            dry_run,
            no_start,
            package,
        } => capabilities.into_iter().try_for_each(|capability| {
            add::add(
                capability,
                name.as_deref(),
                dry_run,
                package.as_deref(),
                debug,
                no_start,
            )
        }),
        Command::Remove {
            capabilities,
            name,
            dry_run,
            force,
            package,
        } => capabilities.into_iter().try_for_each(|capability| {
            add::remove(
                capability,
                name.as_deref(),
                dry_run,
                force,
                package.as_deref(),
                debug,
            )
        }),
        Command::Rename {
            old,
            new,
            dry_run,
            force,
        } => rename::rename(&old, &new, dry_run, force),
        Command::Destroy {
            kind,
            name,
            force,
            package,
        } => generate::destroy(kind, &name, force, package.as_deref()),
        Command::Start { services } => compose::start(&services, debug),
        Command::Stop { services } => compose::stop_cmd(&services, debug),
        Command::Doctor => doctor::doctor(),
        Command::Why { log } => why::why(log.as_deref(), debug),
        Command::Routes { json } => inspect::routes(json),
        Command::Beans { pattern, json } => inspect::beans(pattern.as_deref(), json),
        Command::Db {
            file,
            no_start,
            args,
        } => console::db(file.as_deref(), no_start, &args, debug),
        Command::Console { no_build, args } => console::console(no_build, &args, debug),
        Command::Test { filter } => run::test(filter.as_deref(), debug),
        Command::Build => run::build(debug),
        Command::Clean => run::clean(debug),
        Command::Fmt => run::fmt(debug),
        Command::Check => run::check(debug),
        Command::Mvn { args } => run::mvn(&args, debug),
        Command::Run {
            no_build,
            watch,
            args,
        } => {
            if watch {
                run::watch(debug)
            } else {
                run::run(no_build, &args, debug)
            }
        }
        Command::Completion { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "jails", &mut std::io::stdout());
            Ok(())
        }
    };

    if let Err(err) = result {
        // An empty message means the command has already printed everything
        // the user needs (`doctor` prints a report and then fails only to
        // set the exit code); printing a bare `jails: ` under it would be
        // noise.
        if !err.is_empty() {
            eprintln!("jails: {err}");
        }
        std::process::exit(1);
    }
}
