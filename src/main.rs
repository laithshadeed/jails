mod add;
mod app;
mod compose;
mod config;
mod console;
mod doctor;
mod generate;
mod inspect;
mod java;
mod kafka;
mod migrate;
mod new;
mod pom;
mod process;
mod project;
mod rename;
mod run;
mod spring;
mod sql;
mod template;
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

    /// Run, but write nothing -- print what would change and stop.
    ///
    /// Global on purpose: Rails puts `--pretend` on every generator rather
    /// than on the few that seemed risky, and the value is that you never
    /// have to remember which commands support it. `add`, `remove` and
    /// `rename` also accept `--dry-run`, which means the same thing.
    #[arg(long, short = 'p', global = true)]
    pretend: bool,
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
    /// Plan or apply a generic declarative application manifest
    App {
        #[command(subcommand)]
        command: app::AppCommand,
    },
    /// Generate a scaffold or one small Java/SQL artifact
    ///
    /// FIELDS are `name:type`, with an optional suffix:
    ///
    ///   name:string      required, must not be null
    ///   name:string!     required and must not be blank (text only)
    ///   name:string?     optional -- becomes an Optional<T> component
    ///
    /// Case is the rule. A lowercase type is one jails knows and can build a
    /// sample of: string, int, long, double, boolean, uuid, instant, date,
    /// datetime, bigdecimal, duration, uri, path, zoneid. A capitalised one
    /// is a type your project owns -- passed through verbatim, no import,
    /// same package. `id:String` still works; jails recognises the Java
    /// spelling of its own built-ins.
    ///
    /// Collections are `list<string>` and `map<string,long>`.
    ///
    /// Examples:
    ///   jails g scaffold Payout id:uuid amount:bigdecimal paidAt:instant
    ///   jails g record Money amount:long currency:Currency
    ///   jails g scaffold Note title:string! body:string?    # ! non-blank, ? optional
    ///   jails g sealed Outcome Accepted Rejected
    ///   jails g dto Payout                                  # reads the record on disk
    ///   jails g mig add_payout_index
    ///
    /// A kind whose fields jails cannot sample still gets its test, emitted
    /// whole and @Disabled naming the type -- a guess would not compile, and
    /// silence would drop the coverage.
    // verbatim_doc_comment: clap reflows a doc comment into one
    // paragraph by default, which turns the field-syntax table and the
    // examples into an unreadable run-on.
    #[command(visible_alias = "g", verbatim_doc_comment)]
    Generate {
        kind: ArtifactKind,
        name: String,
        fields: Vec<String>,
        /// Subpackage to place the generated code in, relative to the base
        /// package -- overrides the conventional one for the kind. Pass an
        /// empty string to write straight into the base package.
        #[arg(long)]
        package: Option<String>,
        /// A composite or ordered index for the generated migration, as the
        /// column list Postgres wants. Repeatable.
        ///
        /// Per-column `@index` covers the single-column case; this is for the
        /// ones it cannot spell:
        ///   --index 'customer_id, created_at desc'
        #[arg(long = "index", value_name = "COLUMNS")]
        indexes: Vec<String>,
        /// For `strategy`, the type each implementation examines. For
        /// `usecase`, the existing scaffolded resource the operation creates;
        /// for `query`, the scaffolded resource it reads; for `durable-job`,
        /// the existing generated use case it invokes. For `command`, the
        /// dispatcher to register it in, when the project has more than one.
        ///
        ///   jails g strategy RewardRule Coffee Large --on Transaction --yields Reward
        #[arg(long = "on", value_name = "TYPE")]
        strategy_on: Option<String>,
        /// For `strategy`: what a matching implementation produces. Omit and
        /// the strategy is a predicate returning `boolean`. For
        /// `durable-job`, the resource whose stable id proves completion.
        #[arg(long = "yields", value_name = "TYPE")]
        strategy_yields: Option<String>,
    },
    /// Add one or more capabilities to an existing project: dependencies, code and tests
    ///
    /// A capability is a whole slice, not a dependency line: the artifact in
    /// pom.xml, the code that uses it, a test that proves it works, and where
    /// relevant a compose service and the properties that make it behave.
    /// Re-running one reports what is already there and changes nothing else.
    ///
    /// Examples:
    ///   jails add db                 # postgres, Flyway, Testcontainers, compose
    ///   jails add api                # RFC 9457 problem responses + validation
    ///   jails add db kafka redis     # several at once
    ///   jails add csv --name Ledger  # name the generated class
    ///   jails add security --pretend # see the plan, write nothing
    ///
    /// `jails remove <capability>` is the exact inverse.
    #[command(visible_alias = "a", verbatim_doc_comment)]
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
    /// Apply every capability `jails.toml` declares that is not there yet
    ///
    /// `jails add` records what it applies, so `jails.toml` describes what the
    /// project is made of. `sync` reads it back and makes the project match --
    /// one command for a fresh clone, or for taking a newer jails' output.
    ///
    /// Every capability is idempotent, so a sync over a correct project
    /// changes nothing and says so. `--pretend` answers "what is missing?".
    Sync {
        /// Print what would change without touching the project
        #[arg(long)]
        dry_run: bool,
        /// Write compose.yaml but do not run `docker compose up`
        #[arg(long)]
        no_start: bool,
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
    /// Count files and lines per layer, and the test-to-code ratio
    Stats,
    /// List TODO/FIXME/HACK/XXX comments across the project
    Notes {
        /// Only this tag (e.g. `jails notes fixme`)
        tag: Option<String>,
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
    /// Apply the migrations to a scratch database and report the first failure
    ///
    /// Not a `doctor` check: doctor is read-only by contract, and applying
    /// migrations writes. Doctor can say whether anything will run them;
    /// this says whether they work.
    Migrate {
        /// Apply to a scratch database and drop it. The only mode there is --
        /// jails does not run migrations against a real database, Flyway
        /// does. Accepted so the documented `jails migrate --check` keeps
        /// working; `--check=false` is refused rather than quietly checking
        /// anyway.
        #[arg(long, default_value_t = true)]
        check: bool,
        /// Do not `docker compose up` postgres first
        #[arg(long)]
        no_start: bool,
    },
    /// Send messages to the compose broker and inspect what is on it
    Kafka {
        #[command(subcommand)]
        command: kafka::KafkaCommand,
        /// Do not `docker compose up` kafka first
        #[arg(long)]
        no_start: bool,
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
    /// Prepare this machine for fast runs: container reuse, and what else it needs
    ///
    /// Machine-level, not project-level. The one setting jails cannot write
    /// into a project is the Testcontainers reuse flag: it is read from
    /// `~/.testcontainers.properties` or the environment, and a file on the
    /// classpath does nothing at all -- so a generated `withReuse(true)` is
    /// ignored until this has run, and every test run pays for a fresh
    /// PostgreSQL.
    Setup {
        /// Describe what would change and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Print a shell-completion script: source <(jails completion bash)
    Completion { shell: Shell },
}

/// Returns [`ExitCode`] rather than calling [`std::process::exit`].
///
/// `process::exit` terminates without unwinding, so destructors on the
/// current stack never run. jails holds real resources while a command is in
/// flight -- `migrate` creates a scratch database it is responsible for
/// dropping -- and anything staging a file beside its destination would be in
/// the same position. Returning lets the stack unwind normally first.
fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let debug = cli.debug;
    let pretend = cli.pretend;

    let result = match cli.command {
        Command::About { json } => project::about(json),
        Command::New {
            name,
            deps,
            java,
            no_git,
            no_devtools,
        } => new::new(&name, &deps, &java, !no_git, !no_devtools, debug, pretend),
        Command::NewCli {
            name,
            release,
            no_git,
        } => new::new_cli(&name, &release, !no_git, debug, pretend),
        Command::App { command } => app::run(command, debug, pretend),
        Command::Generate {
            kind,
            name,
            fields,
            package,
            indexes,
            strategy_on,
            strategy_yields,
        } => generate::generate(
            kind,
            &name,
            &fields,
            package.as_deref(),
            &indexes,
            strategy_on.as_deref(),
            strategy_yields.as_deref(),
            pretend,
        ),
        Command::Add {
            capabilities,
            name,
            dry_run,
            no_start,
            package,
        } => add::preflight(&capabilities, name.as_deref(), package.as_deref()).and_then(|()| {
            capabilities.into_iter().try_for_each(|capability| {
                add::add(
                    capability,
                    name.as_deref(),
                    dry_run || pretend,
                    package.as_deref(),
                    debug,
                    no_start,
                )
            })
        }),
        Command::Sync { dry_run, no_start } => add::sync(dry_run || pretend, debug, no_start),
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
                dry_run || pretend,
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
        } => rename::rename(&old, &new, dry_run || pretend, force),
        Command::Destroy {
            kind,
            name,
            force,
            package,
        } => generate::destroy(kind, &name, force, package.as_deref(), pretend),
        Command::Start { services } => compose::start(&services, debug),
        Command::Stop { services } => compose::stop_cmd(&services, debug),
        Command::Doctor => doctor::doctor(),
        Command::Why { log } => why::why(log.as_deref(), debug),
        Command::Stats => inspect::stats(),
        Command::Notes { tag } => inspect::notes(tag.as_deref()),
        Command::Routes { json } => inspect::routes(json),
        Command::Beans { pattern, json } => inspect::beans(pattern.as_deref(), json),
        Command::Migrate { check, no_start } => {
            if !check {
                Err(
                    "`--check` is the only mode jails has: it applies the migrations to a \
                     scratch database and drops it. Applying them for real is Flyway's job, \
                     which the application does at startup.\n\nfix: run `jails migrate`."
                        .to_string(),
                )
            } else {
                migrate::check(no_start, debug)
            }
        }
        Command::Kafka { command, no_start } => kafka::kafka(command, no_start, debug),
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
        Command::Setup { dry_run } => doctor::setup(dry_run || pretend),
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
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
