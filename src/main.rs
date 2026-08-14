mod add;
mod generate;
mod pom;
mod new;
mod run;

use add::Capability;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use generate::ArtifactKind;

pub type Result<T> = std::result::Result<T, String>;

/// Prints the program, args and working directory of a command about to be
/// run, for `--debug`. Called right before every `.status()`/`.spawn()` in
/// run.rs/new.rs.
pub(crate) fn debug_cmd(cmd: &std::process::Command) {
    let program = cmd.get_program().to_string_lossy();
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();
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
#[command(name = "jails", version, about = "A rails-CLI-inspired tool for Spring Boot / plain Maven projects")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Print the mvn/mvnd/java/git/curl commands jails executes
    #[arg(long, global = true)]
    debug: bool,
}

#[derive(Subcommand)]
enum Command {
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
        /// Skip `git init` and the .gitignore it normally sets up
        #[arg(long)]
        no_git: bool,
    },
    /// Generate a scaffold or a single artifact (controller|service|repository|entity|test)
    #[command(visible_alias = "g")]
    Generate {
        kind: ArtifactKind,
        name: String,
        fields: Vec<String>,
    },
    /// Add a capability to an existing project: dependency, code and a test
    #[command(visible_alias = "a")]
    Add {
        capability: Capability,
        /// Base name for the generated class (default: the capability's own)
        #[arg(long)]
        name: Option<String>,
        /// Print what would change without touching the project
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete the file(s) a matching generate call would have created
    #[command(visible_alias = "d")]
    Destroy {
        kind: ArtifactKind,
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// Run the test suite (mvn/mvnd test)
    Test { filter: Option<String> },
    /// Build the project (mvn package)
    Build,
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
    },
    /// Print a shell-completion script: source <(jails completion bash)
    Completion { shell: Shell },
}

fn main() {
    let cli = Cli::parse();
    let debug = cli.debug;

    let result = match cli.command {
        Command::New { name, deps, java, no_git, no_devtools } => {
            new::new(&name, &deps, &java, !no_git, !no_devtools, debug)
        }
        Command::NewCli { name, no_git } => new::new_cli(&name, !no_git, debug),
        Command::Generate { kind, name, fields } => generate::generate(kind, &name, &fields),
        Command::Add { capability, name, dry_run } => add::add(capability, name.as_deref(), dry_run),
        Command::Destroy { kind, name, force } => generate::destroy(kind, &name, force),
        Command::Test { filter } => run::test(filter.as_deref(), debug),
        Command::Build => run::build(debug),
        Command::Run { no_build, watch } => {
            if watch {
                run::watch(debug)
            } else {
                run::run(no_build, debug)
            }
        }
        Command::Completion { shell } => {
            clap_complete::generate(shell, &mut Cli::command(), "jails", &mut std::io::stdout());
            Ok(())
        }
    };

    if let Err(err) = result {
        eprintln!("jails: {err}");
        std::process::exit(1);
    }
}
