mod generate;
mod new;
mod run;

use clap::{Parser, Subcommand};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Parser)]
#[command(name = "jails", about = "A rails-CLI-inspired tool for Spring Boot / plain Maven projects")]
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
    },
    /// Create a new plain Maven CLI project
    NewCli { name: String },
    /// Generate a scaffold or a single artifact (controller|service|repository|entity|test)
    #[command(alias = "g")]
    Generate {
        kind: String,
        name: String,
        fields: Vec<String>,
    },
    /// Delete the file(s) a matching generate call would have created
    #[command(alias = "d")]
    Destroy {
        kind: String,
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// Run the test suite (mvn/mvnd test)
    Test { filter: Option<String> },
    /// Build the project (mvn package)
    Build,
    /// Find, compile and run the project's main class
    Run,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::New { name, deps, java } => new::new(&name, &deps, &java),
        Command::NewCli { name } => new::new_cli(&name),
        Command::Generate { kind, name, fields } => generate::generate(&kind, &name, &fields),
        Command::Destroy { kind, name, force } => generate::destroy(&kind, &name, force),
        Command::Test { filter } => run::test(filter.as_deref()),
        Command::Build => run::build(),
        Command::Run => run::run(),
    };

    if let Err(err) = result {
        eprintln!("jails: {err}");
        std::process::exit(1);
    }
}
