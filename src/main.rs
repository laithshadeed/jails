mod generate;
mod new;
mod run;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use generate::ArtifactKind;

pub type Result<T> = std::result::Result<T, String>;

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
}

#[derive(Subcommand)]
enum Command {
    /// Create a new Spring Boot project via start.spring.io
    New {
        name: String,
        #[arg(long, default_value = "web")]
        deps: String,
        #[arg(long, default_value = "26")]
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

    let result = match cli.command {
        Command::New { name, deps, java, no_git, no_devtools } => {
            new::new(&name, &deps, &java, !no_git, !no_devtools)
        }
        Command::NewCli { name, no_git } => new::new_cli(&name, !no_git),
        Command::Generate { kind, name, fields } => generate::generate(kind, &name, &fields),
        Command::Destroy { kind, name, force } => generate::destroy(kind, &name, force),
        Command::Test { filter } => run::test(filter.as_deref()),
        Command::Build => run::build(),
        Command::Run { no_build, watch } => {
            if watch {
                run::watch()
            } else {
                run::run(no_build)
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
